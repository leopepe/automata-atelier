//! A continuously-polling sensor (ADR 0005).
//!
//! One actor per `SensorSpec`. Each schedules its own poll ticks from
//! `on_start`, so sensors run independently and in parallel — the blocking
//! shell-out runs on `tokio`'s blocking pool, giving real OS-thread
//! parallelism when the host has cores. Readings are reported to the
//! world-state actor; this actor never touches state directly.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use kameo::prelude::*;

use crate::actors::messages::{ApplyReading, Tick};
use crate::actors::world_state::WorldStateActor;
use crate::config::SensorSpec;
use crate::run::read_sensor;

/// Spawn arguments for [`SensorActor`].
pub struct SensorArgs {
    pub spec: Arc<SensorSpec>,
    pub world: ActorRef<WorldStateActor>,
    /// Delay between this sensor's poll ticks. `0` means poll as fast as the
    /// shell-out allows.
    pub interval_ms: u64,
}

/// Polls one sensor command on a fixed cadence.
pub struct SensorActor {
    spec: Arc<SensorSpec>,
    world: ActorRef<WorldStateActor>,
}

impl Actor for SensorActor {
    type Args = SensorArgs;
    type Error = Infallible;

    async fn on_start(args: Self::Args, actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        // Drive the poll loop from a detached task that pings this actor. It
        // exits when the actor goes away (tell returns an error), so the loop
        // is bounded by the actor's lifetime.
        let me = actor_ref;
        let interval = Duration::from_millis(args.interval_ms);
        tokio::spawn(async move {
            loop {
                if me.tell(Tick).send().await.is_err() {
                    break;
                }
                tokio::time::sleep(interval).await;
            }
        });
        Ok(Self {
            spec: args.spec,
            world: args.world,
        })
    }
}

impl Message<Tick> for SensorActor {
    type Reply = ();

    async fn handle(&mut self, _msg: Tick, _ctx: &mut Context<Self, Self::Reply>) {
        let spec = Arc::clone(&self.spec);
        // Subprocess wait is variable-latency I/O → off the async worker.
        // Command failed to spawn (e.g. missing binary) or the blocking task
        // panicked → skip this tick; the next one retries. A permanently-broken
        // sensor leaves its fact unchanged rather than crashing the runtime.
        let Ok(Ok(reading)) = tokio::task::spawn_blocking(move || read_sensor(&spec)).await else {
            return;
        };
        let _ = self.world.tell(ApplyReading(reading)).send().await;
    }
}
