//! The runtime driver (ADR 0005).
//!
//! Builds the actor graph, performs an initial parallel sensor sweep so the
//! first plan sees every sensor's opening reading, wires the cross-references,
//! starts continuous sensing, and then renders events until a terminal outcome
//! or SIGINT. This task is the root of the supervision tree: it owns the actor
//! handles and tears them down gracefully on shutdown (an in-flight action
//! always runs to completion — actions are never aborted mid-flight).

use std::sync::Arc;

use goap_planner::Planner;
use kameo::prelude::*;
use tokio::sync::{mpsc, oneshot};

use crate::actors::executor::{ExecutorActor, ExecutorArgs};
use crate::actors::goal_supervisor::{GoalSupervisorActor, GoalSupervisorArgs};
use crate::actors::messages::{
    ApplyReading, Bootstrap, SetExecutor, SetExecutorSupervisor, SetGoalSupervisor, SetPlanners,
    SetSubscriber, SetWorldState,
};
use crate::actors::planner::{PlannerActor, PlannerArgs};
use crate::actors::sensor::{SensorActor, SensorArgs};
use crate::actors::world_state::{WorldStateActor, WorldStateArgs};
use crate::actors::{RuntimeEvent, RuntimeOutcome};
use crate::config::Config;
use crate::run::{build_actions, build_goal, read_sensor};

/// Tunables for one run of the reactive runtime.
pub struct RuntimeParams {
    /// Facts seeded into the initial state before sensors run.
    pub seed: Vec<String>,
    /// Per-sensor poll cadence in milliseconds (`0` = as fast as work allows).
    pub interval_ms: u64,
    /// Safety cap on actions executed before the run stops.
    pub max_actions: usize,
    /// Keep running past goal-satisfied / no-plan (watcher mode, issue #17).
    pub watch: bool,
}

/// Entry point for the actor runtime.
pub struct AgentRuntime;

impl AgentRuntime {
    /// Drive the reactive runtime to a terminal [`RuntimeOutcome`].
    ///
    /// `on_event` is called for every [`RuntimeEvent`] in render order, with a
    /// final [`RuntimeEvent::Complete`] last. Returns the outcome so the caller
    /// can map it to a process exit code.
    pub async fn run(
        config: &Config,
        params: RuntimeParams,
        mut on_event: impl FnMut(&RuntimeEvent),
    ) -> RuntimeOutcome {
        let (ev_tx, mut ev_rx) = mpsc::unbounded_channel::<RuntimeEvent>();
        let (done_tx, mut done_rx) = oneshot::channel::<RuntimeOutcome>();

        let planner = Arc::new(Planner::new(build_actions(&config.actions)));
        let goal = build_goal(&config.goal);
        let actions = Arc::new(config.actions.clone());

        // 1. World — sole state owner, no subscriber yet.
        let world = WorldStateActor::spawn(WorldStateArgs {
            seed: params.seed.clone(),
            events: ev_tx.clone(),
        });

        // 2. Initial sweep — sequential, in config order. The first plan must
        //    see a deterministic baseline, and some configs declare sensors
        //    with ordering-dependent side effects (a discovery sensor that
        //    stages work a later sensor observes — the issue #19 foot-gun).
        //    Running the *one-time* baseline sweep in declared order preserves
        //    those configs; steady-state sensing below is fully parallel.
        //    Applying before the subscriber is wired means these readings don't
        //    each kick a premature plan — the first plan sees the whole sweep.
        for spec in &config.sensors {
            let spec = spec.clone();
            let read = tokio::task::spawn_blocking(move || read_sensor(&spec)).await;
            if let Ok(Ok(reading)) = read {
                // `ask` so it is applied before we wire the subscriber.
                let _ = world.ask(ApplyReading(reading)).await;
            }
        }

        // 3. Planner, executor, supervisor.
        let planner_ref = PlannerActor::spawn(PlannerArgs { planner, goal });
        let executor_ref = ExecutorActor::spawn(ExecutorArgs {
            actions,
            events: ev_tx.clone(),
            max_actions: params.max_actions,
        });
        let goal_sup = GoalSupervisorActor::spawn(GoalSupervisorArgs {
            events: ev_tx.clone(),
            completion: done_tx,
            watch: params.watch,
        });

        // 4. Wire the graph's cycles (second phase).
        let _ = world.tell(SetSubscriber(goal_sup.clone())).send().await;
        let _ = planner_ref
            .tell(SetGoalSupervisor(goal_sup.clone()))
            .send()
            .await;
        let _ = goal_sup
            .tell(SetPlanners(vec![planner_ref.clone()]))
            .send()
            .await;
        let _ = goal_sup
            .tell(SetExecutor(executor_ref.clone()))
            .send()
            .await;
        let _ = executor_ref.tell(SetWorldState(world.clone())).send().await;
        let _ = executor_ref
            .tell(SetExecutorSupervisor(goal_sup.clone()))
            .send()
            .await;

        // 5. Start continuous sensing.
        let mut sensors = Vec::with_capacity(config.sensors.len());
        for spec in &config.sensors {
            sensors.push(SensorActor::spawn(SensorArgs {
                spec: Arc::new(spec.clone()),
                world: world.clone(),
                interval_ms: params.interval_ms,
            }));
        }

        // 6. Kick the first plan from the swept initial state.
        let _ = world.tell(Bootstrap).send().await;

        // 7. Render events until a terminal outcome or SIGINT.
        let outcome = loop {
            tokio::select! {
                biased;
                res = &mut done_rx => break res.unwrap_or(RuntimeOutcome::Interrupted),
                _ = tokio::signal::ctrl_c() => break RuntimeOutcome::Interrupted,
                maybe = ev_rx.recv() => {
                    if let Some(ev) = maybe {
                        on_event(&ev);
                    }
                }
            }
        };

        // 8. Tear down gracefully, drain any straggler events, emit Complete.
        for sensor in &sensors {
            let _ = sensor.stop_gracefully().await;
        }
        let _ = executor_ref.stop_gracefully().await;
        let _ = planner_ref.stop_gracefully().await;
        let _ = goal_sup.stop_gracefully().await;
        let _ = world.stop_gracefully().await;
        drop(ev_tx);
        while let Ok(ev) = ev_rx.try_recv() {
            on_event(&ev);
        }
        on_event(&RuntimeEvent::Complete {
            outcome: outcome.clone(),
        });
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ActionSpec, Effects, GoalSpec, SensorSpec};

    fn sensor(name: &str) -> SensorSpec {
        SensorSpec {
            name: name.into(),
            cmd: vec!["true".into()],
            on_success: None,
            on_failure: None,
            capture: None,
        }
    }

    fn action(name: &str, cmd: &[&str], requires: &[&str], adds: &[&str]) -> ActionSpec {
        ActionSpec {
            name: name.into(),
            cost: 1.0,
            requires: requires.iter().map(|s| s.to_string()).collect(),
            forbids: Vec::new(),
            adds: adds.iter().map(|s| s.to_string()).collect(),
            removes: Vec::new(),
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            on_failure: None,
        }
    }

    fn params() -> RuntimeParams {
        RuntimeParams {
            seed: Vec::new(),
            interval_ms: 0,
            max_actions: 100,
            watch: false,
        }
    }

    async fn run_collect(config: &Config, p: RuntimeParams) -> (RuntimeOutcome, Vec<RuntimeEvent>) {
        let mut events = Vec::new();
        let outcome = AgentRuntime::run(config, p, |e| events.push(e.clone())).await;
        (outcome, events)
    }

    fn executed(events: &[RuntimeEvent]) -> Vec<(String, bool)> {
        events
            .iter()
            .filter_map(|e| match e {
                RuntimeEvent::Executed { result } => Some((result.name.clone(), result.success)),
                _ => None,
            })
            .collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn drives_three_step_config_to_goal_through_replanning() {
        let config = Config {
            sensors: vec![sensor("heartbeat")],
            actions: vec![
                action("do_a", &["true"], &["heartbeat"], &["a_done"]),
                action("do_b", &["true"], &["a_done"], &["b_done"]),
                action("do_c", &["true"], &["b_done"], &["finished"]),
            ],
            goal: GoalSpec {
                requires: vec!["finished".into()],
                forbids: Vec::new(),
            },
        };
        let (outcome, events) = run_collect(&config, params()).await;
        assert_eq!(outcome, RuntimeOutcome::GoalSatisfied);
        let names: Vec<String> = executed(&events).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["do_a", "do_b", "do_c"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn reports_no_plan_when_goal_unreachable() {
        let config = Config {
            sensors: vec![sensor("heartbeat")],
            actions: vec![action("do_a", &["true"], &["heartbeat"], &["a_done"])],
            goal: GoalSpec {
                requires: vec!["unreachable".into()],
                forbids: Vec::new(),
            },
        };
        let (outcome, _) = run_collect(&config, params()).await;
        assert_eq!(outcome, RuntimeOutcome::NoPlan);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fatal_action_failure_terminates_with_action_failed() {
        let config = Config {
            sensors: vec![sensor("heartbeat")],
            actions: vec![action(
                "do_a",
                &["sh", "-c", "exit 7"],
                &["heartbeat"],
                &["finished"],
            )],
            goal: GoalSpec {
                requires: vec!["finished".into()],
                forbids: Vec::new(),
            },
        };
        let (outcome, _) = run_collect(&config, params()).await;
        match outcome {
            RuntimeOutcome::ActionFailed {
                name, exit_code, ..
            } => {
                assert_eq!(name, "do_a");
                assert_eq!(exit_code, Some(7));
            }
            other => panic!("expected ActionFailed, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn recoverable_failure_replans_through_alternative_path() {
        // `try_fast` fails; its on_failure unlocks the slow path. The reactive
        // replan must route around the failure and reach the goal via `try_slow`.
        let config = Config {
            sensors: vec![sensor("heartbeat")],
            actions: vec![
                ActionSpec {
                    name: "try_fast".into(),
                    cost: 1.0,
                    requires: vec!["heartbeat".into(), "fast".into()],
                    forbids: Vec::new(),
                    adds: vec!["finished".into()],
                    removes: Vec::new(),
                    cmd: Some(vec!["false".into()]),
                    on_failure: Some(Effects {
                        add: vec!["slow".into()],
                        remove: vec!["fast".into()],
                    }),
                },
                ActionSpec {
                    name: "try_slow".into(),
                    cost: 5.0,
                    requires: vec!["slow".into()],
                    forbids: Vec::new(),
                    adds: vec!["finished".into()],
                    removes: Vec::new(),
                    cmd: Some(vec!["true".into()]),
                    on_failure: None,
                },
            ],
            goal: GoalSpec {
                requires: vec!["finished".into()],
                forbids: Vec::new(),
            },
        };
        let p = RuntimeParams {
            seed: vec!["fast".into()],
            interval_ms: 0,
            max_actions: 100,
            watch: false,
        };
        let (outcome, events) = run_collect(&config, p).await;
        assert_eq!(outcome, RuntimeOutcome::GoalSatisfied);
        assert_eq!(
            executed(&events),
            vec![("try_fast".into(), false), ("try_slow".into(), true)],
        );
    }
}
