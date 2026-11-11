//! The validator of a page as an agent sent it: reads each value against the JSON Schema of its
//! kind (`super::kinds`) and names the first failing location as a path. It knows the few schema
//! keywords the kinds use: `type`, `enum`, `properties`, `required`, `additionalProperties` as
//! `false`, `items`, `minItems`, `minLength`, `maxLength`, `minimum`, `maximum` and
//! `exclusiveMinimum`, and the `x-component` mark that sends a value to the schema of the kind its
//! `type` names. It does not read `const`, which only a kind's `type` uses: the kind is chosen by
//! that `type`, so it always matches. A string with a `minLength` of at least one is also refused
//! when it is only whitespace.

use std::fmt;

use serde_json::Value;

use super::kinds::{kind_names, page_schema, schema_named, KINDS, PAGE_SCHEMA_NAME};

/// What is wrong with a page: where, and what, and which schema the failing part is to match.
#[derive(Debug, Clone, PartialEq)]
pub struct PageContentError {
    /// The location as a JSON path from the page object, such as
    /// `body[2].children[0].options[1].value`; empty for the page object itself.
    pub path: String,
    pub message: String,
    /// The kind of the component the failure is in, whose schema explains it; none when it is in
    /// the page object itself.
    pub kind: Option<&'static str>,
}

impl PageContentError {
    pub(super) fn new(
        path: impl Into<String>,
        message: impl Into<String>,
        kind: Option<&'static str>,
    ) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
            kind,
        }
    }

    /// The name [`schema_named`] gives the schema this failure is to be read against.
    pub fn schema_name(&self) -> &'static str {
        self.kind.unwrap_or(PAGE_SCHEMA_NAME)
    }

    /// The schema of the failing component kind, or of the page object.
    pub fn schema(&self) -> Value {
        schema_named(self.schema_name()).unwrap_or_else(page_schema)
    }
}

impl fmt::Display for PageContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "the page: {}", self.message)
        } else {
            write!(f, "`{}`: {}", self.path, self.message)
        }
    }
}

impl std::error::Error for PageContentError {}

/// Checks `page` against the page schema, and every component in it against its kind's.
pub(super) fn check_page(page: &Value) -> Result<(), PageContentError> {
    check_node(page, &page_schema(), "", None)
}

pub(super) fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

pub(super) fn index(path: &str, at: usize) -> String {
    format!("{path}[{at}]")
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn is_of_type(value: &Value, wanted: &str) -> bool {
    match wanted {
        "integer" => value.as_number().is_some_and(|n| n.is_i64() || n.is_u64()),
        other => type_name(value) == other,
    }
}

fn describe_types(wanted: &Value) -> String {
    match wanted {
        Value::Array(types) => types
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        other => other.as_str().unwrap_or_default().to_string(),
    }
}

fn list(values: &[Value]) -> String {
    values
        .iter()
        .map(|value| match value {
            Value::String(text) => format!("`{text}`"),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn check_node(
    value: &Value,
    schema: &Value,
    path: &str,
    kind: Option<&'static str>,
) -> Result<(), PageContentError> {
    let fail = |message: String| Err(PageContentError::new(path, message, kind));
    if let Some(wanted) = schema.get("type") {
        let fits = match wanted {
            Value::Array(types) => types
                .iter()
                .filter_map(Value::as_str)
                .any(|wanted| is_of_type(value, wanted)),
            other => other
                .as_str()
                .is_some_and(|wanted| is_of_type(value, wanted)),
        };
        if !fits {
            return fail(format!(
                "must be {}, not {}",
                describe_types(wanted),
                type_name(value)
            ));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array) {
        if !allowed.contains(value) {
            return fail(format!("must be one of {}", list(allowed)));
        }
    }
    if schema.get("x-component").is_some() {
        return check_component(value, path, kind);
    }
    match value {
        Value::String(text) => {
            if let Some(least) = schema.get("minLength").and_then(Value::as_u64) {
                let blank = least >= 1 && text.trim().is_empty();
                if blank || (text.chars().count() as u64) < least {
                    return fail(if least == 1 {
                        "must not be blank".to_string()
                    } else {
                        format!("must have at least {least} characters")
                    });
                }
            }
            if let Some(most) = schema.get("maxLength").and_then(Value::as_u64) {
                if text.chars().count() as u64 > most {
                    return fail(format!("must have at most {most} characters"));
                }
            }
        }
        Value::Number(number) => {
            let number = number.as_f64().unwrap_or(f64::NAN);
            if let Some(least) = schema.get("minimum").and_then(Value::as_f64) {
                if number < least {
                    return fail(format!("must be at least {least}"));
                }
            }
            if let Some(greatest) = schema.get("maximum").and_then(Value::as_f64) {
                if number > greatest {
                    return fail(format!("must be at most {greatest}"));
                }
            }
            if let Some(above) = schema.get("exclusiveMinimum").and_then(Value::as_f64) {
                if number <= above {
                    return fail(format!("must be greater than {above}"));
                }
            }
        }
        Value::Array(items) => {
            if let Some(least) = schema.get("minItems").and_then(Value::as_u64) {
                if (items.len() as u64) < least {
                    return fail(format!("needs at least {least} item(s)"));
                }
            }
            if let Some(item_schema) = schema.get("items") {
                for (at, item) in items.iter().enumerate() {
                    check_node(item, item_schema, &index(path, at), kind)?;
                }
            }
        }
        Value::Object(object) => check_object(object, schema, path, kind)?,
        _ => {}
    }
    Ok(())
}

fn check_object(
    object: &serde_json::Map<String, Value>,
    schema: &Value,
    path: &str,
    kind: Option<&'static str>,
) -> Result<(), PageContentError> {
    let properties = schema.get("properties").and_then(Value::as_object);
    if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
        if let Some(properties) = properties {
            if let Some(unknown) = object.keys().find(|key| !properties.contains_key(*key)) {
                let allowed: Vec<String> =
                    properties.keys().map(|key| format!("`{key}`")).collect();
                return Err(PageContentError::new(
                    join(path, unknown),
                    format!("unknown property; allowed: {}", allowed.join(", ")),
                    kind,
                ));
            }
        }
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for name in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(name) {
                return Err(PageContentError::new(
                    path,
                    format!("missing required property `{name}`"),
                    kind,
                ));
            }
        }
    }
    if let Some(properties) = properties {
        for (name, property_schema) in properties {
            if let Some(property) = object.get(name) {
                check_node(property, property_schema, &join(path, name), kind)?;
            }
        }
    }
    Ok(())
}

/// A component read against the schema of the kind its `type` names. A failure about the
/// component as a whole is the enclosing kind's (`container`), whose schema says where it goes.
fn check_component(
    value: &Value,
    path: &str,
    container: Option<&'static str>,
) -> Result<(), PageContentError> {
    let Some(object) = value.as_object() else {
        return Err(PageContentError::new(
            path,
            format!("a component must be an object, not {}", type_name(value)),
            container,
        ));
    };
    let kinds = || {
        kind_names()
            .map(|name| format!("`{name}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let Some(named) = object.get("type") else {
        return Err(PageContentError::new(
            path,
            format!("missing `type`, which names the kind: {}", kinds()),
            container,
        ));
    };
    let kind = named
        .as_str()
        .and_then(|named| KINDS.iter().find(|kind| kind.name == named));
    let Some(kind) = kind else {
        return Err(PageContentError::new(
            join(path, "type"),
            format!(
                "{} is not a kind of component; the kinds are {}",
                named,
                kinds()
            ),
            container,
        ));
    };
    check_node(value, &(kind.schema)(), path, Some(kind.name))
}
