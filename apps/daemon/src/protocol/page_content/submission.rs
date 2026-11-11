//! What a form on a page may be submitted with.

use serde_json::Value;

use super::rules::gather;
use super::{PageContent, Submission, SubmittedValue};

/// A submission is an object whose keys are the page's fields and submitters, with a string, a
/// number, a boolean or an array of strings for each. The pairs stay in the order sent. A page
/// with no submitter has no form, so nothing can be submitted from it.
pub(super) fn check_submission(content: &PageContent, data: &Value) -> Result<Submission, String> {
    let names = gather(content).map_err(|error| error.to_string())?;
    if !names.has_submitter {
        return Err("this page has no form: it has no `submit` or `choices`".to_string());
    }
    let Some(fields) = data.as_object() else {
        return Err("the submission must be an object of field values".to_string());
    };
    for (key, value) in fields {
        if !names.inputs.contains_key(key.as_str()) && !names.submitters.contains_key(key.as_str())
        {
            let known: Vec<String> = names
                .inputs
                .keys()
                .chain(names.submitters.keys())
                .map(|name| format!("`{name}`"))
                .collect();
            return Err(format!(
                "`{key}` is not a field of this page; its fields are {}",
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ));
        }
        if serde_json::from_value::<SubmittedValue>(value.clone()).is_err() {
            return Err(format!(
                "`{key}` must be a string, a number, a boolean or an array of strings"
            ));
        }
    }
    Ok(fields.clone())
}
