//! The sole owner of the world state (ADR 0005).
//!
//! Holds `goap-planner`'s [`State`] and the ADR-0003 [`Values`] map. Sensor
//! readings and action effects flow in as messages; the only thing that ever
//! mutates the world is this actor handling one message at a time. Readers take
//! a [`WorldSnapshot`]. This is the actor model's single-owner rule, not a
//! revival of the old central serializer — nothing here plans or executes.

use std::collections::BTreeSet;
use std::convert::Infallible;

use goap_planner::State;
use kameo::prelude::*;

use crate::actors::goal_supervisor::GoalSupervisorActor;
use crate::actors::messages::{
    ApplyActionEffects, ApplyReading, Bootstrap, SetSubscriber, Snapshot, StateChanged,
    WorldSnapshot,
};
use crate::actors::{EventSink, RuntimeEvent};
use crate::run::{Values, apply_reading};

/// Spawn arguments for [`WorldStateActor`].
pub struct WorldStateArgs {
    /// Facts seeded into the initial state before any sensor runs.
    pub seed: Vec<String>,
    /// Event channel for `sensed` events.
    pub events: EventSink,
}

/// Owns the world; edge-triggers replanning on change.
pub struct WorldStateActor {
    state: State,
    values: Values,
    subscriber: Option<ActorRef<GoalSupervisorActor>>,
    events: EventSink,
}

impl WorldStateActor {
    fn snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            state: self.state.clone(),
            values: self.values.clone(),
        }
    }

    fn sorted_facts(&self) -> Vec<String> {
        let mut v: Vec<String> = self.state.facts().map(String::from).collect();
        v.sort();
        v
    }

    /// Push the current snapshot to the subscriber, if one is wired.
    async fn notify(&self) {
        if let Some(sub) = &self.subscriber {
            let _ = sub.tell(StateChanged(self.snapshot())).send().await;
        }
    }
}

impl Actor for WorldStateActor {
    type Args = WorldStateArgs;
    type Error = Infallible;

    async fn on_start(args: Self::Args, _actor_ref: ActorRef<Self>) -> Result<Self, Self::Error> {
        Ok(Self {
            state: State::from_facts(args.seed),
            values: Values::new(),
            subscriber: None,
            events: args.events,
        })
    }
}

impl Message<ApplyReading> for WorldStateActor {
    /// Whether the reading changed the world (and so triggered a replan).
    type Reply = bool;

    async fn handle(&mut self, msg: ApplyReading, _ctx: &mut Context<Self, Self::Reply>) -> bool {
        let reading = msg.0;

        let facts_before: BTreeSet<String> = self.state.facts().map(String::from).collect();
        let values_before = self.values.clone();
        apply_reading(&reading, &mut self.state, &mut self.values);
        let facts_after: BTreeSet<String> = self.state.facts().map(String::from).collect();
        let changed = facts_before != facts_after || values_before != self.values;

        let _ = self.events.send(RuntimeEvent::Sensed {
            sensor: reading.name,
            success: reading.success,
            added: reading.added,
            removed: reading.removed,
            captured: reading.captured_value,
            changed,
            state: self.sorted_facts(),
            values: self.values.clone(),
        });

        if changed {
            self.notify().await;
        }
        changed
    }
}

impl Message<ApplyActionEffects> for WorldStateActor {
    type Reply = ();

    async fn handle(&mut self, msg: ApplyActionEffects, _ctx: &mut Context<Self, Self::Reply>) {
        // Removing a fact drops its value too — ADR 0003's atomic-remove rule.
        for fact in &msg.removes {
            self.state.remove(fact);
            self.values.remove(fact);
        }
        for fact in &msg.adds {
            self.state.insert(fact.clone());
        }
        // Always notify: a finished action means the executor needs the next
        // step, even if the optimistic effects happened to be a no-op.
        self.notify().await;
    }
}

impl Message<Bootstrap> for WorldStateActor {
    type Reply = ();

    async fn handle(&mut self, _msg: Bootstrap, _ctx: &mut Context<Self, Self::Reply>) {
        self.notify().await;
    }
}

impl Message<Snapshot> for WorldStateActor {
    type Reply = WorldSnapshot;

    async fn handle(
        &mut self,
        _msg: Snapshot,
        _ctx: &mut Context<Self, Self::Reply>,
    ) -> WorldSnapshot {
        self.snapshot()
    }
}

impl Message<SetSubscriber> for WorldStateActor {
    type Reply = ();

    async fn handle(&mut self, msg: SetSubscriber, _ctx: &mut Context<Self, Self::Reply>) {
        self.subscriber = Some(msg.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::SensorReading;
    use tokio::sync::mpsc;

    fn reading(
        name: &str,
        added: &[&str],
        removed: &[&str],
        captured: Option<&str>,
    ) -> SensorReading {
        SensorReading {
            name: name.into(),
            success: true,
            added: added.iter().map(std::string::ToString::to_string).collect(),
            removed: removed
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            captured_value: captured.map(std::string::ToString::to_string),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn apply_reading_reports_change_then_no_change() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let world = WorldStateActor::spawn(WorldStateArgs {
            seed: Vec::new(),
            events: tx,
        });
        // First time a fact is added → changed.
        let first = world
            .ask(ApplyReading(reading("x", &["x"], &[], None)))
            .await
            .unwrap();
        assert!(first, "adding a new fact must report a change");
        // Same fact again → no change (edge-trigger: no replan).
        let second = world
            .ask(ApplyReading(reading("x", &["x"], &[], None)))
            .await
            .unwrap();
        assert!(
            !second,
            "re-observing the same fact must not report a change"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn snapshot_reflects_applied_readings_and_captured_values() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let world = WorldStateActor::spawn(WorldStateArgs {
            seed: vec!["seed".into()],
            events: tx,
        });
        let _ = world
            .ask(ApplyReading(reading(
                "target",
                &["target"],
                &[],
                Some("v1"),
            )))
            .await
            .unwrap();
        let snap = world.ask(Snapshot).await.unwrap();
        assert!(snap.state.contains("seed"));
        assert!(snap.state.contains("target"));
        assert_eq!(snap.values.get("target").map(String::as_str), Some("v1"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn removing_a_fact_drops_its_value() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let world = WorldStateActor::spawn(WorldStateArgs {
            seed: Vec::new(),
            events: tx,
        });
        let _ = world
            .ask(ApplyReading(reading(
                "target",
                &["target"],
                &[],
                Some("v1"),
            )))
            .await
            .unwrap();
        // A failing-style reading that removes the fact also drops its value
        // (ADR 0003 atomic-remove).
        let _ = world
            .ask(ApplyReading(reading("target", &[], &["target"], None)))
            .await
            .unwrap();
        let snap = world.ask(Snapshot).await.unwrap();
        assert!(!snap.state.contains("target"));
        assert!(!snap.values.contains_key("target"));
    }
}
