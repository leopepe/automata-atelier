use rustc_hash::FxHashSet;

use crate::state::State;

/// An action transforms a [`State`] when its preconditions hold.
///
/// Effects are applied as: remove `remove_effects` first, then insert
/// `add_effects`. Preconditions are pure conjunctions — every fact in
/// `preconditions` must be present in the source state.
#[derive(Clone, Debug)]
pub struct Action {
    pub name: String,
    pub cost: f64,
    pub preconditions: FxHashSet<String>,
    pub add_effects: FxHashSet<String>,
    pub remove_effects: FxHashSet<String>,
}

impl Action {
    pub fn new(name: impl Into<String>, cost: f64) -> Self {
        Self {
            name: name.into(),
            cost,
            preconditions: FxHashSet::default(),
            add_effects: FxHashSet::default(),
            remove_effects: FxHashSet::default(),
        }
    }

    pub fn requires(mut self, fact: impl Into<String>) -> Self {
        self.preconditions.insert(fact.into());
        self
    }

    pub fn adds(mut self, fact: impl Into<String>) -> Self {
        self.add_effects.insert(fact.into());
        self
    }

    pub fn removes(mut self, fact: impl Into<String>) -> Self {
        self.remove_effects.insert(fact.into());
        self
    }

    pub fn applicable(&self, state: &State) -> bool {
        self.preconditions.iter().all(|p| state.contains(p))
    }

    pub fn apply(&self, state: &State) -> State {
        let mut next = state.clone();
        for fact in &self.remove_effects {
            next.remove(fact);
        }
        for fact in &self.add_effects {
            next.insert(fact.clone());
        }
        next
    }
}
