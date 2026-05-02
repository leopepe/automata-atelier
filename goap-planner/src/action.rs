use rustc_hash::FxHashSet;

use crate::state::State;

/// An action transforms a [`State`] when its preconditions hold.
///
/// Effects are applied as: remove `remove_effects` first, then insert
/// `add_effects`. Preconditions are conjunctions — every fact in
/// `preconditions` must be present in the source state and every fact
/// in `forbidden` must be absent.
#[derive(Clone, Debug)]
pub struct Action {
    pub name: String,
    pub cost: f64,
    pub preconditions: FxHashSet<String>,
    /// Negative preconditions: facts whose presence disables the action.
    /// Mirrors [`crate::Goal::forbids`]. Empty by default — actions
    /// without `forbids` behave exactly as before.
    pub forbidden: FxHashSet<String>,
    pub add_effects: FxHashSet<String>,
    pub remove_effects: FxHashSet<String>,
}

impl Action {
    pub fn new(name: impl Into<String>, cost: f64) -> Self {
        Self {
            name: name.into(),
            cost,
            preconditions: FxHashSet::default(),
            forbidden: FxHashSet::default(),
            add_effects: FxHashSet::default(),
            remove_effects: FxHashSet::default(),
        }
    }

    pub fn requires(mut self, fact: impl Into<String>) -> Self {
        self.preconditions.insert(fact.into());
        self
    }

    /// Add a negative precondition. The action only fires when the
    /// given fact is **absent** from the source state.
    ///
    /// Mirrors [`crate::Goal::forbids`]. Multiple `.forbids(...)` calls
    /// accumulate; all listed facts must be absent for the action to
    /// be applicable.
    ///
    /// # Examples
    ///
    /// ```
    /// use goap_planner::{Action, State};
    ///
    /// let eject = Action::new("eject_now", 1.0)
    ///     .requires("audit_sealed")
    ///     .forbids("pendrive_mounted")
    ///     .adds("eject_done");
    ///
    /// // Audit sealed AND pendrive mounted — action NOT applicable.
    /// assert!(!eject.applicable(&State::from_facts(["audit_sealed", "pendrive_mounted"])));
    /// // Audit sealed AND pendrive unmounted — action IS applicable.
    /// assert!(eject.applicable(&State::from_facts(["audit_sealed"])));
    /// ```
    pub fn forbids(mut self, fact: impl Into<String>) -> Self {
        self.forbidden.insert(fact.into());
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

    /// `true` iff every fact in `preconditions` is present in `state`
    /// **and** every fact in `forbidden` is absent from it.
    ///
    /// Hot-path note: actions without negative preconditions (the common
    /// case) skip the second iteration entirely via an `is_empty()`
    /// short-circuit. Building the empty-set iterator was visible in the
    /// `ops/action/applicable_*` micro-benches at ~17 % overhead on PR
    /// #33's CI run; the short-circuit restores parity while keeping
    /// the contract symmetric for actions that do use `forbids`.
    pub fn applicable(&self, state: &State) -> bool {
        if !self.preconditions.iter().all(|p| state.contains(p)) {
            return false;
        }
        if self.forbidden.is_empty() {
            return true;
        }
        self.forbidden.iter().all(|f| !state.contains(f))
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
