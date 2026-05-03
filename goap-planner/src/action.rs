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
    /// Mirrors [`crate::Goal::forbids`]. `None` is the common case —
    /// actions without any `forbids` calls. Stored behind a `Box` so
    /// `Option<Box<…>>` is niche-optimised to a single pointer-sized
    /// field; this keeps `Action` only 8 bytes larger than before
    /// (instead of growing by a full empty `FxHashSet`'s ~32 bytes)
    /// and lets the hot path in `applicable()` short-circuit on a
    /// pointer-null check without touching any allocation.
    pub forbidden: Option<Box<FxHashSet<String>>>,
    pub add_effects: FxHashSet<String>,
    pub remove_effects: FxHashSet<String>,
}

impl Action {
    pub fn new(name: impl Into<String>, cost: f64) -> Self {
        Self {
            name: name.into(),
            cost,
            preconditions: FxHashSet::default(),
            forbidden: None,
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
        self.forbidden
            .get_or_insert_with(|| Box::new(FxHashSet::default()))
            .insert(fact.into());
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
    /// Hot-path note: the common case (action declares no `forbids`)
    /// short-circuits on a pointer-null check — `forbidden` is
    /// `Option<Box<…>>` exactly so `None` is an inline 8-byte null.
    /// This avoids the cache-line load that an inline empty
    /// `FxHashSet` field would impose on every call, which otherwise
    /// shows up as ~16 % overhead on the `ops/action/applicable_*`
    /// micro-benches.
    pub fn applicable(&self, state: &State) -> bool {
        if !self.preconditions.iter().all(|p| state.contains(p)) {
            return false;
        }
        match &self.forbidden {
            None => true,
            Some(forbidden) => forbidden.iter().all(|f| !state.contains(f)),
        }
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
