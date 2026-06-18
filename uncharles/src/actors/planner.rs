//! Wraps the one and only [`goap_planner::Planner`] (ADR 0005).
//!
//! On every [`PlanRequest`] it runs `plan()` off the async workers (CPU-bound
//! sync work must not block a `tokio` worker — see the workspace coding
//! guidelines) and reports the result to the goal supervisor. Bursts of
//! changes are coalesced: while a plan is in flight, only the most recent
//! request is kept, and exactly one follow-up plan runs when it lands. No
//! "is the remaining plan still valid" guessing — replanning is the mechanism.

use std::convert::Infallible;
use std::sync::Arc;

use goap_planner::{Goal, Planner};
use kameo::prelude::*;

use crate::actors::goal_supervisor::GoalSupervisorActor;
use crate::actors::messages::{
    PlanComputed, PlanReady, PlanRequest, SetGoalSupervisor, WorldSnapshot,
};

/// Spawn arguments for [`PlannerActor`].
pub struct PlannerArgs {
    pub planner: Arc<Planner>,
    pub goal: Goal,
}

/// Owns a planner + goal; computes plans off-thread and coalesces requests.
pub struct PlannerActor {
    planner: Arc<Planner>,
    goal: Goal,
    supervisor: Option<ActorRef<GoalSupervisorActor>>,
    busy: bool,
    pending: Option<WorldSnapshot>,
}

impl PlannerActor {
    /// Launch an off-thread `plan()` for `snapshot`, reporting back to `self`.
    fn spawn_plan(&self, snapshot: WorldSnapshot, me: ActorRef<Self>) {
        let planner = Arc::clone(&self.planner);
        let goal = self.goal.clone();
        let snapshot_for_reply = snapshot.clone();
        tokio::spawn(async move {
            let state = snapshot.state.clone();
            let result = tokio::task::spawn_blocking(move || {
                planner.plan(&state, &goal).map_err(|e| e.to_string())
            })
            .await
            .unwrap_or_else(|e| Err(format!("planner task panicked: {e}")));
            let _ = me
                .tell(PlanComputed {
                    result,
                    snapshot: snapshot_for_reply,
                })
                .send()
                .await;
        });
    }
}

impl Actor for PlannerActor {
    type Args = PlannerArgs;
    type Error = Infallible;

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            planner: args.planner,
            goal: args.goal,
            supervisor: None,
            busy: false,
            pending: None,
        })
    }
}

impl Message<PlanRequest> for PlannerActor {
    type Reply = ();

    async fn handle(&mut self, msg: PlanRequest, ctx: &mut Context<Self, Self::Reply>) {
        if self.busy {
            // Coalesce: keep only the freshest snapshot.
            self.pending = Some(msg.0);
        } else {
            self.busy = true;
            self.spawn_plan(msg.0, ctx.actor_ref().clone());
        }
    }
}

impl Message<PlanComputed> for PlannerActor {
    type Reply = ();

    async fn handle(&mut self, msg: PlanComputed, ctx: &mut Context<Self, Self::Reply>) {
        if let Some(sup) = &self.supervisor {
            let _ = sup
                .tell(PlanReady {
                    result: msg.result,
                    snapshot: msg.snapshot,
                })
                .send()
                .await;
        }
        // Drain a coalesced request, if one arrived while we were planning.
        if let Some(next) = self.pending.take() {
            self.spawn_plan(next, ctx.actor_ref().clone());
        } else {
            self.busy = false;
        }
    }
}

impl Message<SetGoalSupervisor> for PlannerActor {
    type Reply = ();

    async fn handle(&mut self, msg: SetGoalSupervisor, _ctx: &mut Context<Self, Self::Reply>) {
        self.supervisor = Some(msg.0);
    }
}
