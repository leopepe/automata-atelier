use rustc_hash::FxHashSet;

/// World state expressed as a set of string-tagged predicates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    facts: FxHashSet<String>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_facts<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            facts: iter.into_iter().map(Into::into).collect(),
        }
    }

    pub fn contains(&self, fact: &str) -> bool {
        self.facts.contains(fact)
    }

    pub fn insert(&mut self, fact: impl Into<String>) {
        self.facts.insert(fact.into());
    }

    pub fn remove(&mut self, fact: &str) {
        self.facts.remove(fact);
    }

    pub fn facts(&self) -> impl Iterator<Item = &str> {
        self.facts.iter().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.facts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }

    /// Canonical, hashable representation used as the node label in the
    /// planning graph. Facts are sorted and joined by `\x1F` (ASCII unit
    /// separator) to keep the encoding unambiguous for arbitrary fact names.
    pub(crate) fn signature(&self) -> String {
        let mut sorted: Vec<&str> = self.facts.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted.join("\x1F")
    }
}
