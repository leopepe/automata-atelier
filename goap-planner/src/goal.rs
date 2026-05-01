use rustc_hash::FxHashSet;

use crate::state::State;

/// A target predicate over [`State`]: the planner halts at any state where
/// every fact in `required` is present and every fact in `forbidden` is absent.
#[derive(Clone, Debug, Default)]
pub struct Goal {
    required: FxHashSet<String>,
    forbidden: FxHashSet<String>,
}

impl Goal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn requires(mut self, fact: impl Into<String>) -> Self {
        self.required.insert(fact.into());
        self
    }

    pub fn forbids(mut self, fact: impl Into<String>) -> Self {
        self.forbidden.insert(fact.into());
        self
    }

    pub fn satisfied_by(&self, state: &State) -> bool {
        self.required.iter().all(|f| state.contains(f))
            && self.forbidden.iter().all(|f| !state.contains(f))
    }
}
