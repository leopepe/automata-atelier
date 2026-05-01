/// A computed action sequence from initial state to a goal-satisfying state.
///
/// `steps` is the ordered list of [`crate::Action`] names to execute;
/// `cost` is the sum of action costs along the path.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub steps: Vec<String>,
    pub cost: f64,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }
}
