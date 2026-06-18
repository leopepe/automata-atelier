//! Message types exchanged between the runtime's actors (ADR 0005).
//!
//! Every payload that carries planning data uses `goap-planner`'s own types
//! (`State`, `Plan`) — there are deliberately no parallel "actor" state or plan
//! types. Wiring messages (`Set*`) inject the cross-references that can't be
//! known at spawn time, resolving the actor graph's cycles in a second phase.

use goap_planner::{Plan, State};
use kameo::prelude::*;

use crate::actors::RuntimeOutcome;
use crate::actors::executor::ExecutorActor;
use crate::actors::goal_supervisor::GoalSupervisorActor;
use crate::actors::planner::PlannerActor;
use crate::actors::world_state::WorldStateActor;
use crate::config::ActionSpec;
use crate::run::{ActionResult, SensorReading, Values};

/// Immutable snapshot of the world handed to planners and the executor.
#[derive(Clone, Debug, Reply)]
pub struct WorldSnapshot {
    pub state: State,
    pub values: Values,
}

// --- WorldStateActor inbox ------------------------------------------------

/// A sensor reported. Applied to the world; notifies the subscriber only if it
/// actually changes state (sensor noise must not trigger replans).
#[derive(Debug)]
pub struct ApplyReading(pub SensorReading);

/// An executed action's effects. Always notifies the subscriber — the executor
/// needs the post-effect state to plan its next step.
#[derive(Debug)]
pub struct ApplyActionEffects {
    pub adds: Vec<String>,
    pub removes: Vec<String>,
}

/// Force a snapshot push to the subscriber, regardless of change. Kicks the
/// first plan once wiring is complete (covers zero-sensor configs too).
#[derive(Debug)]
pub struct Bootstrap;

/// Read-only snapshot of the current world (used by tests and tooling).
#[derive(Debug)]
pub struct Snapshot;

/// Inject the change subscriber (the goal supervisor).
#[derive(Debug)]
pub struct SetSubscriber(pub ActorRef<GoalSupervisorActor>);

// --- PlannerActor inbox ---------------------------------------------------

/// Plan for this snapshot. Coalesced: while a plan is in flight, the latest
/// request is stashed and run once the current one completes.
#[derive(Debug)]
pub struct PlanRequest(pub WorldSnapshot);

/// Internal: an off-thread `plan()` call finished.
#[derive(Debug)]
pub struct PlanComputed {
    pub result: Result<Option<Plan>, String>,
    pub snapshot: WorldSnapshot,
}

/// Inject the goal supervisor the planner reports results to.
#[derive(Debug)]
pub struct SetGoalSupervisor(pub ActorRef<GoalSupervisorActor>);

// --- GoalSupervisorActor inbox --------------------------------------------

/// The world changed; replan.
#[derive(Debug)]
pub struct StateChanged(pub WorldSnapshot);

/// A planner produced a result for arbitration.
#[derive(Debug)]
pub struct PlanReady {
    pub result: Result<Option<Plan>, String>,
    pub snapshot: WorldSnapshot,
}

/// The executor reached a terminal condition (failure, cap, exec error).
#[derive(Debug)]
pub struct Terminate(pub RuntimeOutcome);

/// Inject the planner pool (one per goal; one for now).
#[derive(Debug)]
pub struct SetPlanners(pub Vec<ActorRef<PlannerActor>>);

/// Inject the executor plans are dispatched to.
#[derive(Debug)]
pub struct SetExecutor(pub ActorRef<ExecutorActor>);

// --- ExecutorActor inbox --------------------------------------------------

/// Adopt a plan to execute. Always non-empty (the supervisor handles the
/// empty/none terminal cases). The snapshot supplies values for env injection.
#[derive(Debug)]
pub struct AdoptPlan {
    pub plan: Plan,
    pub snapshot: WorldSnapshot,
}

/// Internal: an off-thread action `cmd` finished.
#[derive(Debug)]
pub struct ActionFinished {
    pub result: Result<ActionResult, String>,
    pub spec: ActionSpec,
}

/// Inject the world-state actor effects are reported to.
#[derive(Debug)]
pub struct SetWorldState(pub ActorRef<WorldStateActor>);

/// Inject the supervisor terminal conditions are reported to.
#[derive(Debug)]
pub struct SetExecutorSupervisor(pub ActorRef<GoalSupervisorActor>);

// --- SensorActor inbox ----------------------------------------------------

/// Poll tick: read the sensor and report.
#[derive(Debug)]
pub struct Tick;
