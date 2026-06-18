//! Routes changes to planner(s) and arbitrates which plan executes (ADR 0005).
//!
//! Today it drives a single goal: forward each [`StateChanged`] to the planner,
//! and on [`PlanReady`] decide the terminal cases (no plan / goal satisfied) or
//! dispatch a runnable plan to the executor. It is the seat where multi-goal
//! arbitration lands later — N independent planners, results compared here —
//! never joint planning inside `goap-planner`.
//!
//! In `watch` mode the goal-satisfied and no-plan results are not terminal: the
//! runtime keeps sensing and replans when the world next changes (issue #17).

use std::convert::Infallible;

use kameo::prelude::*;
use tokio::sync::oneshot;

use crate::actors::executor::ExecutorActor;
use crate::actors::messages::{
    AdoptPlan, PlanReady, PlanRequest, SetExecutor, SetPlanners, StateChanged, Terminate,
};
use crate::actors::planner::PlannerActor;
use crate::actors::{EventSink, RuntimeEvent, RuntimeOutcome};

/// Spawn arguments for [`GoalSupervisorActor`].
pub struct GoalSupervisorArgs {
    pub events: EventSink,
    /// Fires once with the terminal outcome. The driver awaits it.
    pub completion: oneshot::Sender<RuntimeOutcome>,
    /// Keep running past goal-satisfied / no-plan (watcher mode).
    pub watch: bool,
}

/// Arbitrates plans for the executor; owns the terminal-outcome channel.
pub struct GoalSupervisorActor {
    planners: Vec<ActorRef<PlannerActor>>,
    executor: Option<ActorRef<ExecutorActor>>,
    events: EventSink,
    completion: Option<oneshot::Sender<RuntimeOutcome>>,
    watch: bool,
    done: bool,
}

impl GoalSupervisorActor {
    /// Fire the terminal outcome exactly once.
    fn finish(&mut self, outcome: RuntimeOutcome) {
        if self.done {
            return;
        }
        self.done = true;
        if let Some(tx) = self.completion.take() {
            let _ = tx.send(outcome);
        }
    }
}

impl Actor for GoalSupervisorActor {
    type Args = GoalSupervisorArgs;
    type Error = Infallible;

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            planners: Vec::new(),
            executor: None,
            events: args.events,
            completion: Some(args.completion),
            watch: args.watch,
            done: false,
        })
    }
}

impl Message<StateChanged> for GoalSupervisorActor {
    type Reply = ();

    async fn handle(&mut self, msg: StateChanged, _ctx: &mut Context<Self, Self::Reply>) {
        if self.done {
            return;
        }
        for planner in &self.planners {
            let _ = planner.tell(PlanRequest(msg.0.clone())).send().await;
        }
    }
}

impl Message<PlanReady> for GoalSupervisorActor {
    type Reply = ();

    async fn handle(&mut self, msg: PlanReady, _ctx: &mut Context<Self, Self::Reply>) {
        if self.done {
            return;
        }
        match msg.result {
            Err(message) => {
                let _ = self.events.send(RuntimeEvent::Planned { plan: None });
                self.finish(RuntimeOutcome::Error { message });
            }
            Ok(None) => {
                let _ = self.events.send(RuntimeEvent::Planned { plan: None });
                if !self.watch {
                    self.finish(RuntimeOutcome::NoPlan);
                }
            }
            Ok(Some(plan)) if plan.steps.is_empty() => {
                let _ = self.events.send(RuntimeEvent::Planned { plan: Some(plan) });
                if !self.watch {
                    self.finish(RuntimeOutcome::GoalSatisfied);
                }
            }
            Ok(Some(plan)) => {
                let _ = self.events.send(RuntimeEvent::Planned {
                    plan: Some(plan.clone()),
                });
                if let Some(executor) = &self.executor {
                    let _ = executor
                        .tell(AdoptPlan {
                            plan,
                            snapshot: msg.snapshot,
                        })
                        .send()
                        .await;
                }
            }
        }
    }
}

impl Message<Terminate> for GoalSupervisorActor {
    type Reply = ();

    async fn handle(&mut self, msg: Terminate, _ctx: &mut Context<Self, Self::Reply>) {
        self.finish(msg.0);
    }
}

impl Message<SetPlanners> for GoalSupervisorActor {
    type Reply = ();

    async fn handle(&mut self, msg: SetPlanners, _ctx: &mut Context<Self, Self::Reply>) {
        self.planners = msg.0;
    }
}

impl Message<SetExecutor> for GoalSupervisorActor {
    type Reply = ();

    async fn handle(&mut self, msg: SetExecutor, _ctx: &mut Context<Self, Self::Reply>) {
        self.executor = Some(msg.0);
    }
}
