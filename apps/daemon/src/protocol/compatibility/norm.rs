//! What a schema node accepts, read as a set of values by kind: the model `compare.rs` compares.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde_json::{json, Map, Value};

/// The keywords the comparison reads, and those it knowingly does not compare.
pub(super) const READ: &[&str] = &[
    "$ref",
    "allOf",
    "oneOf",
    "anyOf",
    "type",
    "enum",
    "const",
    "format",
    "minimum",
    "maximum",
    "properties",
    "required",
    "additionalProperties",
    "propertyNames",
    "items",
];
pub(super) const IGNORED: &[&str] = &[
    "$schema",
    "$comment",
    "$defs",
    "title",
    "description",
    "default",
    "uniqueItems",
    "x-octoboard-protocol",
];

/// The values a schema node accepts, by kind.
#[derive(Clone, Default)]
pub(super) struct Norm {
    /// Anything at all.
    pub(super) any: bool,
    pub(super) null: bool,
    pub(super) boolean: bool,
    /// Numbers that need not be whole.
    pub(super) number: bool,
    pub(super) integer: Option<Range>,
    /// A string; the values it is limited to, or `None` for any.
    pub(super) string: Option<Option<BTreeSet<String>>>,
    /// An array; the schema of its items, or `None` for anything.
    pub(super) array: Option<Option<Value>>,
    /// The shapes an object may take, one per alternative of the schema.
    pub(super) objects: Vec<Obj>,
}

#[derive(Clone, Copy)]
pub(super) struct Range {
    pub(super) low: i128,
    pub(super) high: i128,
}

impl Range {
    pub(super) fn contains(&self, other: &Range) -> bool {
        self.low <= other.low && other.high <= self.high
    }
}

impl fmt::Display for Range {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let end = |bound: i128, unbounded: i128| {
            if bound == unbounded {
                "unbounded".to_string()
            } else {
                bound.to_string()
            }
        };
        write!(
            formatter,
            "{}..={}",
            end(self.low, i128::MIN),
            end(self.high, i128::MAX)
        )
    }
}

#[derive(Clone, Default)]
pub(super) struct Obj {
    pub(super) props: BTreeMap<String, Value>,
    pub(super) required: BTreeSet<String>,
    /// The schema of a map's values.
    pub(super) additional: Option<Value>,
    /// The schema of a map's keys.
    pub(super) names: Option<Value>,
}

impl Obj {
    /// What tells a variant from its siblings: every required property whose schema is one
    /// string, by name. A tagged enum nested in another (an event whose `state` has its own
    /// variants) is tagged by both properties, so the inner variants match one by one. Empty
    /// for an untagged object.
    pub(super) fn tag(&self) -> Vec<(String, String)> {
        self.props
            .iter()
            .filter_map(|(name, schema)| {
                let value = schema.get("const")?.as_str()?;
                self.required
                    .contains(name)
                    .then(|| (name.clone(), value.to_string()))
            })
            .collect()
    }

    /// The object both describe: every property of either, and everything either requires.
    pub(super) fn merge(&self, other: &Obj) -> Obj {
        let both = |first: &Option<Value>, second: &Option<Value>| match (first, second) {
            (Some(first), Some(second)) => Some(json!({ "allOf": [first, second] })),
            (one, other) => one.clone().or_else(|| other.clone()),
        };
        let mut props = self.props.clone();
        for (name, schema) in &other.props {
            let merged = props
                .remove(name)
                .map_or(schema.clone(), |own| json!({ "allOf": [own, schema] }));
            props.insert(name.clone(), merged);
        }
        Obj {
            props,
            required: self.required.union(&other.required).cloned().collect(),
            additional: both(&self.additional, &other.additional),
            names: both(&self.names, &other.names),
        }
    }
}

impl Norm {
    pub(super) fn everything() -> Norm {
        Norm {
            any: true,
            ..Norm::default()
        }
    }

    /// What both accept.
    pub(super) fn intersect(self, other: Norm) -> Norm {
        if self.any {
            return other;
        }
        if other.any {
            return self;
        }
        let integer = match (self.integer, other.integer) {
            (Some(first), Some(second)) => Some(Range {
                low: first.low.max(second.low),
                high: first.high.min(second.high),
            }),
            (Some(range), None) if other.number => Some(range),
            (None, Some(range)) if self.number => Some(range),
            _ => None,
        };
        let string = match (self.string, other.string) {
            (Some(first), Some(second)) => Some(match (first, second) {
                (Some(first), Some(second)) => Some(first.intersection(&second).cloned().collect()),
                (one, other) => one.or(other),
            }),
            _ => None,
        };
        let array = match (self.array, other.array) {
            (Some(first), Some(second)) => Some(match (first, second) {
                (Some(first), Some(second)) => Some(json!({ "allOf": [first, second] })),
                (one, other) => one.or(other),
            }),
            _ => None,
        };
        let mut objects = Vec::new();
        for first in &self.objects {
            for second in &other.objects {
                objects.push(first.merge(second));
            }
        }
        Norm {
            any: false,
            null: self.null && other.null,
            boolean: self.boolean && other.boolean,
            number: self.number && other.number,
            integer,
            string,
            array,
            objects,
        }
    }

    /// What either accepts.
    pub(super) fn union(mut self, other: Norm) -> Norm {
        if self.any || other.any {
            return Norm::everything();
        }
        self.null |= other.null;
        self.boolean |= other.boolean;
        self.number |= other.number;
        self.integer = match (self.integer, other.integer) {
            (Some(first), Some(second)) => Some(Range {
                low: first.low.min(second.low),
                high: first.high.max(second.high),
            }),
            (one, other) => one.or(other),
        };
        self.string = match (self.string, other.string) {
            (Some(first), Some(second)) => Some(match (first, second) {
                (Some(first), Some(second)) => Some(first.union(&second).cloned().collect()),
                _ => None,
            }),
            (one, other) => one.or(other),
        };
        self.array = match (self.array, other.array) {
            (Some(first), Some(second)) => Some(match (first, second) {
                (Some(first), Some(second)) => Some(json!({ "anyOf": [first, second] })),
                _ => None,
            }),
            (one, other) => one.or(other),
        };
        self.objects.extend(other.objects);
        self
    }
}

/// Folds a node's keywords into the values it accepts: `Err` names what the guard cannot read.
pub(super) fn normalize(
    definitions: Option<&Map<String, Value>>,
    node: &Value,
) -> Result<Norm, String> {
    let object = match node {
        Value::Bool(true) => return Ok(Norm::everything()),
        Value::Object(object) => object,
        other => return Err(format!("expected a schema and found {other}")),
    };
    if let Some(key) = object
        .keys()
        .find(|key| !READ.contains(&key.as_str()) && !IGNORED.contains(&key.as_str()))
    {
        return Err(format!(
            "the keyword `{key}` is not one the compatibility guard reads: teach it to \
             apps/daemon/src/protocol/compatibility/norm.rs"
        ));
    }
    let mut accepted: Option<Norm> = None;
    let mut and = |norm: Norm| {
        accepted = Some(match accepted.take() {
            Some(so_far) => so_far.intersect(norm),
            None => norm,
        });
    };
    if let Some(reference) = object.get("$ref") {
        let target = reference
            .as_str()
            .and_then(|reference| reference.strip_prefix("#/$defs/"))
            .and_then(|name| definitions?.get(name))
            .ok_or_else(|| format!("the reference {reference} names no definition"))?;
        and(normalize(definitions, target)?);
    }
    if let Some(Value::Array(parts)) = object.get("allOf") {
        for part in parts {
            and(normalize(definitions, part)?);
        }
    }
    for keyword in ["oneOf", "anyOf"] {
        if let Some(Value::Array(alternatives)) = object.get(keyword) {
            let mut union = Norm::default();
            for alternative in alternatives {
                union = union.union(normalize(definitions, alternative)?);
            }
            and(union);
        }
    }
    if let Some(own) = own_keywords(object)? {
        and(own);
    }
    Ok(accepted.unwrap_or_else(Norm::everything))
}

/// What a node's own `type`, `enum`, `const` and object, array and number keywords accept, or
/// `None` when it has none.
pub(super) fn own_keywords(object: &Map<String, Value>) -> Result<Option<Norm>, String> {
    let has = |keywords: &[&str]| keywords.iter().any(|keyword| object.contains_key(*keyword));
    let objects = has(&[
        "properties",
        "required",
        "additionalProperties",
        "propertyNames",
    ]);
    let numbers = has(&["format", "minimum", "maximum"]);
    let mut norm = Norm::default();
    if let Some(values) = object.get("enum") {
        let Value::Array(values) = values else {
            return Err(format!("an enum is an array, not {values}"));
        };
        for value in values {
            norm = norm.union(constant(value)?);
        }
        return Ok(Some(norm));
    }
    if let Some(value) = object.get("const") {
        return Ok(Some(constant(value)?));
    }
    let types: Vec<&str> = match object.get("type") {
        Some(Value::String(name)) => vec![name],
        Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).collect(),
        _ => {
            let mut implied = Vec::new();
            if objects {
                implied.push("object");
            }
            if object.contains_key("items") {
                implied.push("array");
            }
            if numbers {
                implied.push("integer");
            }
            implied
        }
    };
    if types.is_empty() {
        return Ok(None);
    }
    for name in types {
        match name {
            "null" => norm.null = true,
            "boolean" => norm.boolean = true,
            "number" => norm.number = true,
            "integer" => norm.integer = Some(range(object)),
            "string" => norm.string = Some(None),
            "array" => norm.array = Some(object.get("items").cloned()),
            "object" => norm.objects.push(Obj {
                props: match object.get("properties") {
                    Some(Value::Object(props)) => props.clone().into_iter().collect(),
                    _ => BTreeMap::new(),
                },
                required: match object.get("required") {
                    Some(Value::Array(names)) => names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect(),
                    _ => BTreeSet::new(),
                },
                additional: match object.get("additionalProperties") {
                    None | Some(Value::Bool(true)) => None,
                    Some(schema @ Value::Object(_)) => Some(schema.clone()),
                    Some(other) => {
                        return Err(format!(
                            "additionalProperties {other} is not one the guard reads"
                        ))
                    }
                },
                names: object.get("propertyNames").cloned(),
            }),
            other => return Err(format!("the type `{other}` is not one the guard reads")),
        }
    }
    Ok(Some(norm))
}

/// The value of one `enum` entry or a `const`.
pub(super) fn constant(value: &Value) -> Result<Norm, String> {
    let mut norm = Norm::default();
    match value {
        Value::Null => norm.null = true,
        Value::String(text) => norm.string = Some(Some(BTreeSet::from([text.clone()]))),
        other => {
            return Err(format!(
                "the constant {other} is not one the guard reads: only null and strings are"
            ))
        }
    }
    Ok(norm)
}

/// The integers a node allows: its `format`'s width, narrowed by `minimum` and `maximum`.
pub(super) fn range(object: &Map<String, Value>) -> Range {
    let (mut low, mut high) = match object.get("format").and_then(Value::as_str) {
        Some(format) => format_bounds(format),
        None => (i128::MIN, i128::MAX),
    };
    let bound = |keyword: &str| {
        let number = object.get(keyword)?.as_number()?;
        number
            .as_i64()
            .map(i128::from)
            .or_else(|| number.as_u64().map(i128::from))
            .or_else(|| number.as_f64().map(|float| float as i128))
    };
    if let Some(minimum) = bound("minimum") {
        low = low.max(minimum);
    }
    if let Some(maximum) = bound("maximum") {
        high = high.min(maximum);
    }
    Range { low, high }
}

/// The integers a `format` schemars writes holds; `uint` and `int` are the pointer-sized types of
/// the 64-bit platforms the daemon ships on.
pub(super) fn format_bounds(format: &str) -> (i128, i128) {
    let (signed, bits) = match format {
        "uint" => (false, 64),
        "int" => (true, 64),
        _ => match format
            .strip_prefix("uint")
            .map(|bits| (false, bits))
            .or_else(|| format.strip_prefix("int").map(|bits| (true, bits)))
            .and_then(|(signed, bits)| Some((signed, bits.parse::<u32>().ok()?)))
        {
            Some(found) => found,
            None => return (i128::MIN, i128::MAX),
        },
    };
    if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    }
}
