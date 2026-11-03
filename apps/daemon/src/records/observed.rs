//! The model and reasoning effort an agent's own records show a session running with. Only the two
//! values are kept. Which record says them and in which field is each provider's own knowledge,
//! which folds what a record names into an [`Observed`] (`RecordStore::fold_line`,
//! `RecordStore::observe_summary`); reading the records is the collector's business
//! (`collector.rs`).

use serde_json::Value;

/// What the records have shown so far, as the agent spells it. The last record that names a value
/// wins, and a record that names none leaves the last one in place, except where `effort_unset`
/// says otherwise.
#[derive(Default)]
pub struct Observed {
    pub model: Option<String>,
    pub effort: Option<String>,
    /// The last record named the model and, by naming no effort, said that none is set, so an
    /// effort shown earlier no longer holds ([`Self::unset_effort`]).
    pub effort_unset: bool,
}

impl Observed {
    /// Takes what one record names: each value it names replaces the last, and one it does not
    /// name leaves the last in place.
    pub fn fold(&mut self, model: Option<String>, effort: Option<String>) {
        self.model = model.or(self.model.take());
        if effort.is_some() {
            self.effort = effort;
            self.effort_unset = false;
        }
    }

    /// Takes a record that names `model` and, by naming no effort, says that none is set.
    pub fn unset_effort(&mut self, model: String) {
        self.model = Some(model);
        self.effort = None;
        self.effort_unset = true;
    }
}

/// The text `value` holds at `key`, when it is not empty.
pub(crate) fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
