//! Actor-based reactive runtime (ADR 0005).
//!
//! Replaces the synchronous sense → plan → act loop with a topology of
//! [`kameo`] actors running on a multi-threaded `tokio` runtime:
//!
//! ```text
//!   SensorActor×N ──ApplyReading──▶ WorldStateActor ──StateChanged──▶ GoalSupervisorActor
//!                                         ▲                                    │
//!                                         │                            PlanRequest │ PlanReady
//!                                  ApplyActionEffects                          ▼
//!                                         │                              PlannerActor
//!                                   ExecutorActor ◀────────AdoptPlan───────────┘
//! ```
//!
//! - **Sensors** run continuously and in parallel, each on its own poll
//!   cadence, shelling out off-thread and reporting readings.
//! - **`WorldState`** is the sole owner of [`goap_planner::State`] and the
//!   ADR-0003 `Values` map. It edge-triggers a replan only when a reading
//!   actually changes the world (action effects always trigger one — the
//!   executor needs the next step).
//! - **Planner** wraps a single [`goap_planner::Planner`], runs `plan()` off
//!   the async workers via `spawn_blocking`, and coalesces bursts of changes
//!   into one in-flight plan.
//! - **Executor** runs the freshest plan one action at a time, never aborting
//!   mid-action, and feeds optimistic effects back to the world.
//! - **`GoalSupervisor`** routes changes to planner(s) and arbitrates which plan
//!   reaches the executor — the seat where multi-goal arbitration lands later.
//!
//! `goap-planner` is untouched: its `State`, `Goal`, `Action`, `Plan`, and
//! `Planner` types are the payloads that travel through the messages here.

pub mod executor;
pub mod goal_supervisor;
pub mod messages;
pub mod planner;
pub mod runtime;
pub mod sensor;
pub mod world_state;

use goap_planner::Plan;

use crate::run::{ActionResult, Values};

pub use runtime::{AgentRuntime, RuntimeParams};

/// Sender half of the runtime's event channel. Every actor holds a clone and
/// pushes [`RuntimeEvent`]s for the driver task to render (JSON/NDJSON/pretty).
pub type EventSink = tokio::sync::mpsc::UnboundedSender<RuntimeEvent>;

/// An observable moment in the reactive runtime, surfaced to the operator.
///
/// Mirrors the old loop's `sensed`/`planned`/`executed`/`complete` event shape
/// so the CLI's structured output stays recognisable, but each `Sensed` is now
/// per-sensor (sensors no longer run as one batched sweep).
#[derive(Debug, Clone)]
pub enum RuntimeEvent {
    /// One sensor reported. `changed` is whether this reading moved the world
    /// state (and therefore whether it triggered a replan).
    Sensed {
        sensor: String,
        success: bool,
        added: Vec<String>,
        removed: Vec<String>,
        captured: Option<String>,
        changed: bool,
        state: Vec<String>,
        values: Values,
    },
    /// The planner produced a result. `None` = no plan exists; `Some(empty)` =
    /// goal already satisfied; `Some(steps)` = a path to execute.
    Planned { plan: Option<Plan> },
    /// An action finished executing.
    Executed { result: ActionResult },
    /// The run reached a terminal state. Emitted once, last, by the driver.
    Complete { outcome: RuntimeOutcome },
}

/// Why the reactive runtime stopped. Drives the process exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeOutcome {
    /// The goal is satisfied (planner returned an empty plan). Exit 0.
    GoalSatisfied,
    /// No plan exists from the current state. Exit 1.
    NoPlan,
    /// An action's `cmd` exited non-zero and it had no `on_failure` clause.
    /// Exit 1.
    ActionFailed {
        name: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    /// The executed-action safety cap was hit. Exit 1.
    MaxActionsReached { max: usize },
    /// SIGINT drained the runtime. Exit 1.
    Interrupted,
    /// A planner error, an action that failed to spawn, or a planner naming an
    /// action absent from the config. Exit 2.
    Error { message: String },
}
