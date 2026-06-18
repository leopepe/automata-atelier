//! Runs the freshest plan, one action at a time (ADR 0005).
//!
//! The executor holds the latest adopted plan. It runs exactly one action (the
//! plan's first step) off-thread, never aborting an in-flight action, then
//! feeds the optimistic effects back to the world. Applying effects triggers a
//! replan, whose fresh plan arrives as the next [`AdoptPlan`] — so between
//! actions the executor always picks up the newest plan, never a stale tail.

use std::convert::Infallible;
use std::sync::Arc;

use kameo::prelude::*;

use crate::actors::goal_supervisor::GoalSupervisorActor;
use crate::actors::messages::{
    ActionFinished, AdoptPlan, ApplyActionEffects, SetExecutorSupervisor, SetWorldState, Terminate,
};
use crate::actors::world_state::WorldStateActor;
use crate::actors::{EventSink, RuntimeEvent, RuntimeOutcome};
use crate::config::ActionSpec;
use crate::run::{execute_action, find_action_spec};

/// Spawn arguments for [`ExecutorActor`].
pub struct ExecutorArgs {
    pub actions: Arc<Vec<ActionSpec>>,
    pub events: EventSink,
    /// Safety cap on the number of actions executed before the run is stopped.
    pub max_actions: usize,
}

/// Executes plans, one action per replan cycle.
pub struct ExecutorActor {
    actions: Arc<Vec<ActionSpec>>,
    world: Option<ActorRef<WorldStateActor>>,
    supervisor: Option<ActorRef<GoalSupervisorActor>>,
    events: EventSink,
    /// The freshest non-empty plan + the snapshot it was planned from.
    latest: Option<AdoptPlan>,
    /// True while an action is running off-thread.
    busy: bool,
    executed: usize,
    max_actions: usize,
}

impl ExecutorActor {
    async fn terminate(&self, outcome: RuntimeOutcome) {
        if let Some(sup) = &self.supervisor {
            let _ = sup.tell(Terminate(outcome)).send().await;
        }
    }

    /// Start the first step of the latest plan, if idle and one exists.
    async fn try_run_next(&mut self, me: ActorRef<Self>) {
        if self.busy {
            return;
        }
        let Some(adopt) = self.latest.take() else {
            return;
        };

        if self.executed >= self.max_actions {
            self.terminate(RuntimeOutcome::MaxActionsReached {
                max: self.max_actions,
            })
            .await;
            return;
        }

        // The supervisor only ever sends non-empty plans.
        let step = adopt.plan.steps[0].clone();
        let spec = match find_action_spec(&self.actions, &step) {
            Ok(spec) => spec.clone(),
            Err(e) => {
                self.terminate(RuntimeOutcome::Error {
                    message: e.to_string(),
                })
                .await;
                return;
            }
        };

        self.busy = true;
        let values = adopt.snapshot.values.clone();
        let spec_for_run = spec.clone();
        tokio::spawn(async move {
            // Subprocess wait is variable-latency I/O → off the async worker.
            let result =
                tokio::task::spawn_blocking(move || execute_action(&spec_for_run, &values))
                    .await
                    .map_err(|e| format!("action task panicked: {e}"))
                    .and_then(|r| r.map_err(|e| e.to_string()));
            let _ = me.tell(ActionFinished { result, spec }).send().await;
        });
    }
}

impl Actor for ExecutorActor {
    type Args = ExecutorArgs;
    type Error = Infallible;

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            actions: args.actions,
            world: None,
            supervisor: None,
            events: args.events,
            latest: None,
            busy: false,
            executed: 0,
            max_actions: args.max_actions,
        })
    }
}

impl Message<AdoptPlan> for ExecutorActor {
    type Reply = ();

    async fn handle(&mut self, msg: AdoptPlan, ctx: &mut Context<Self, Self::Reply>) {
        // Keep only the freshest plan; if mid-action, this supersedes whatever
        // we'd have run next.
        self.latest = Some(msg);
        let me = ctx.actor_ref().clone();
        self.try_run_next(me).await;
    }
}

impl Message<ActionFinished> for ExecutorActor {
    type Reply = ();

    async fn handle(&mut self, msg: ActionFinished, _ctx: &mut Context<Self, Self::Reply>) {
        self.busy = false;
        // Discard any plan that arrived mid-action: it was computed from
        // pre-effect state. The post-effect replan below yields the fresh one.
        self.latest = None;

        let result = match msg.result {
            Ok(result) => result,
            Err(message) => {
                // The action failed to even spawn — a hard, terminal error.
                self.terminate(RuntimeOutcome::Error { message }).await;
                return;
            }
        };

        let _ = self.events.send(RuntimeEvent::Executed {
            result: result.clone(),
        });
        self.executed += 1;

        let effects = if result.success {
            // Optimistically apply the action's declared effects; sensors will
            // correct the world if reality disagrees.
            Some((msg.spec.adds.clone(), msg.spec.removes.clone()))
        } else if let Some(on_failure) = &msg.spec.on_failure {
            // Recoverable failure: apply the on_failure effects and let the
            // next replan route around it. The action's own adds/removes are
            // the success-path contract and are deliberately skipped.
            Some((on_failure.add.clone(), on_failure.remove.clone()))
        } else {
            // Fatal failure.
            self.terminate(RuntimeOutcome::ActionFailed {
                name: result.name,
                exit_code: result.exit_code,
                stderr: result.stderr,
            })
            .await;
            return;
        };

        if let (Some((adds, removes)), Some(world)) = (effects, &self.world) {
            let _ = world
                .tell(ApplyActionEffects { adds, removes })
                .send()
                .await;
        }
        // No auto-run here: we wait for the post-effect replan's AdoptPlan,
        // guaranteeing the next action is chosen from post-effect state.
    }
}

impl Message<SetWorldState> for ExecutorActor {
    type Reply = ();

    async fn handle(&mut self, msg: SetWorldState, _ctx: &mut Context<Self, Self::Reply>) {
        self.world = Some(msg.0);
    }
}

impl Message<SetExecutorSupervisor> for ExecutorActor {
    type Reply = ();

    async fn handle(&mut self, msg: SetExecutorSupervisor, _ctx: &mut Context<Self, Self::Reply>) {
        self.supervisor = Some(msg.0);
    }
}
