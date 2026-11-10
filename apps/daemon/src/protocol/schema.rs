//! The schema contract test: the wire types rendered as JSON Schema (draft 2020-12) and compared
//! with the committed files of `contracts/protocol/`, the language-neutral description native
//! clients are generated from.
//!
//! The wire types derive `JsonSchema` under `cfg(test)` beside `TS` (see `bindings.rs`), so the
//! release binary carries nothing of it, and a doc comment is the description of its type, field or
//! variant. There is one file per channel and direction, each a schema of one frame on that channel
//! with every named type it reaches as a `$defs` entry. A file is rendered with the serde contract
//! of its direction, what a client sends as the daemon reads it and what the daemon sends as it
//! writes it, so a type both sides send can differ between the files: a field with a default may be
//! left out of a request but is always in an event.
//!
//! Optionality and `null` follow the TypeScript bindings, and `crosscheck.rs` holds the two to
//! each other. A property that may be left out is not `null` unless its field says so with
//! [`nullable`], where the protocol lets a client send a `null`, whether as a value of its own or
//! as the same as leaving the key out, or the daemon writes one: a `Present` field
//! (`present_option`), where `null` clears and leaving the key out leaves alone, read or written;
//! the fields the bindings mark `#[ts(optional = nullable)]` or render as `Partial<…>` of a
//! nullable value; and a field of a record a client sends back as it was given, `null` included. A
//! request field the bindings render as `T | null` without `?` says so with [`required_nullable`],
//! since serde would let it be left out. An output field is optional exactly where it is skipped
//! (`skip_serializing_if`), and is then given `#[ts(optional)]` so that the bindings do not add a
//! `null` the daemon never writes. Where the Rust type says less than the protocol, the field
//! narrows its schema with `#[schemars(extend(…))]`, as `#[ts(type = …)]` does its TypeScript.
//!
//! The schema follows serde where the bindings say less than it: a request field with a serde
//! default and no `ts` attribute may be left out here, though the bindings render it required (the
//! allowed differences of `crosscheck.rs`). A request's per-agent object of `Option` values allows
//! `null` values here, as its description says, where the bindings' `Partial<…>` does not; the
//! cross-check compares properties, not the values of a map.
//!
//! A property is often a `$ref` with sibling keywords (a description, a narrowing `enum`), read
//! with the semantics of draft 2019-09 and later: the siblings apply together with the reference.
//!
//! A doc comment is copied into the description as it is, so the rule of `bindings.rs` on links
//! holds here too.

use std::path::PathBuf;

use schemars::generate::SchemaSettings;
use schemars::transform::{transform_subschemas, Transform};
use schemars::{json_schema, JsonSchema, Schema, SchemaGenerator};
use serde_json::{Map, Value};

use super::{
    BrowseRequest, Change, Code, Event, PerAgent, Request, TermControl, TermEvent, GENERATION,
};

/// Where the files are committed. The test writes them instead of comparing with them under the
/// variable the TypeScript bindings are written under.
const DIRECTORY: &str = "../../contracts/protocol";
const FILE_SUFFIX: &str = ".schema.json";

const REGENERATE: &str = "OCTOBOARD_WRITE_BINDINGS=1 cargo test -p octoboardd protocol::schema";

/// The keyword a field's [`nullable`] or [`required_nullable`] leaves on its schema for
/// [`WireRules`], which takes it off again.
const NULLABLE_MARK: &str = "x-octoboard-nullable";
const REQUIRED_MARK: &str = "x-octoboard-required";

/// A field that may be left out and may also be `null`, where the protocol lets a client send a
/// `null`, whether as a value of its own or as the same as leaving the key out, or where the daemon
/// writes one: named by `#[schemars(transform = schema::nullable)]`.
pub fn nullable(schema: &mut Schema) {
    schema.insert(NULLABLE_MARK.to_string(), true.into());
}

/// A request field a client always sends, `null` included, though serde would read it as `null`
/// when left out: named by `#[schemars(transform = schema::required_nullable)]`.
pub fn required_nullable(schema: &mut Schema) {
    schema.insert(REQUIRED_MARK.to_string(), true.into());
}

/// A code is a plain string on the wire, as in the TypeScript bindings: a newer daemon may send a
/// code a client does not know, so the set of codes is not part of the contract.
impl JsonSchema for Code {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Code".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string" })
    }
}

/// An object keyed by agent id. Read, a provider left out takes the default value and an id
/// naming no registered provider is ignored; written, every registered provider has an entry.
impl<T: JsonSchema> JsonSchema for PerAgent<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("PerAgent_for_{}", T::schema_name()).into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        format!("PerAgent<{}>", T::schema_id()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "object",
            "propertyNames": generator.subschema_for::<super::AgentId>(),
            "additionalProperties": generator.subschema_for::<T>(),
        })
    }
}

/// An update's value for one agent's setting: `null` clears it, and the key being left out, which
/// leaves it alone, is the object's own optionality.
impl<T: JsonSchema> JsonSchema for Change<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("Change_for_{}", T::schema_name()).into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        format!("Change<{}>", T::schema_id()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        generator.subschema_for::<Option<T>>()
    }
}

/// The type of a `#[serde(flatten)]` field, kept a definition of its own that the flattening type
/// names with `allOf`, as the bindings intersect it, where schemars would copy its properties in.
/// Named by `#[schemars(with = "schema::Flattened<T>")]` on the field.
pub struct Flattened<T>(std::marker::PhantomData<T>);

impl<T: JsonSchema> JsonSchema for Flattened<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        format!("Flattened_{}", T::schema_name()).into()
    }

    fn schema_id() -> std::borrow::Cow<'static, str> {
        format!("Flattened<{}>", T::schema_id()).into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        json_schema!({ "allOf": [generator.subschema_for::<T>()] })
    }
}

/// Applies the rules of optionality and `null` above to every object schema: takes `null` off a
/// property that may be left out unless its field is [`nullable`], and lists a
/// [`required_nullable`] one as required.
struct WireRules;

impl Transform for WireRules {
    fn transform(&mut self, schema: &mut Schema) {
        if let Some(object) = schema.as_object_mut() {
            // A default is what the daemon's own types build: a `PerAgent` default would list the
            // providers of the build that renders the file, and an `Option`'s `null` is not on
            // the wire where the key may be left out. The descriptions say what absence means.
            object.remove("default");
            apply_property_rules(object);
        }
        transform_subschemas(self, schema);
    }
}

fn apply_property_rules(object: &mut Map<String, Value>) {
    let listed: Vec<String> = match object.get("required") {
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(|name| name.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let Some(Value::Object(properties)) = object.get_mut("properties") else {
        return;
    };
    let mut required: Vec<String> = Vec::new();
    for (name, property) in properties.iter_mut() {
        let Value::Object(property) = property else {
            continue;
        };
        let nullable = property.remove(NULLABLE_MARK).is_some();
        if property.remove(REQUIRED_MARK).is_some() {
            if !listed.contains(name) {
                required.push(name.clone());
            }
        } else if !nullable && !listed.contains(name) {
            remove_null(property);
        }
    }
    if !required.is_empty() {
        let names = object
            .entry("required")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(names) = names {
            names.extend(required.into_iter().map(Value::String));
        }
    }
}

/// Undoes what schemars adds to make an `Option` nullable: a `null` alternative in `anyOf`, a
/// `"null"` type, or a `null` in `enum`.
fn remove_null(property: &mut Map<String, Value>) {
    let is_null = |schema: &Value| schema.get("type").and_then(Value::as_str) == Some("null");
    if let Some(Value::Array(alternatives)) = property.get_mut("anyOf") {
        if alternatives.iter().any(is_null) {
            alternatives.retain(|alternative| !is_null(alternative));
            if alternatives.len() == 1 {
                let Some(Value::Object(only)) = alternatives.pop() else {
                    panic!("an anyOf alternative is a schema object");
                };
                property.remove("anyOf");
                for (key, value) in only {
                    property.entry(key).or_insert(value);
                }
            }
        }
    }
    if let Some(Value::Array(types)) = property.get_mut("type") {
        types.retain(|name| name != "null");
        if types.len() == 1 {
            let only = types.pop().expect("one type is left");
            property.insert("type".to_string(), only);
        }
    }
    if let Some(Value::Array(values)) = property.get_mut("enum") {
        if values.contains(&Value::Null) {
            values.retain(|value| !value.is_null());
            if values.len() == 1 {
                let only = values.pop().expect("one value is left");
                property.remove("enum");
                property.insert("const".to_string(), only);
            }
        }
    }
}

/// A channel and direction: one committed file.
struct Document {
    file: &'static str,
    channel: &'static str,
    direction: Direction,
    title: &'static str,
    description: &'static str,
    /// The schema of one frame, naming the definitions it reaches.
    root: fn(&mut SchemaGenerator) -> Schema,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Direction {
    /// From the client to the daemon: read by the daemon.
    Input,
    /// From the daemon to the client: written by the daemon.
    Output,
}

const DOCUMENTS: &[Document] = &[
    Document {
        file: "control-input.schema.json",
        channel: "control",
        direction: Direction::Input,
        title: "Octoboard control socket: client to daemon",
        description: "One text frame a client sends on `/ws/control`: a request, or a browse \
                      request, told apart by `type`. See `apps/daemon/PROTOCOL.md`.",
        root: |generator| {
            json_schema!({
                "oneOf": [
                    generator.subschema_for::<Request>(),
                    generator.subschema_for::<BrowseRequest>(),
                ],
            })
        },
    },
    Document {
        file: "control-output.schema.json",
        channel: "control",
        direction: Direction::Output,
        title: "Octoboard control socket: daemon to client",
        description: "One text frame the daemon sends on `/ws/control`. See \
                      `apps/daemon/PROTOCOL.md`.",
        root: |generator| generator.subschema_for::<Event>(),
    },
    Document {
        file: "terminal-input.schema.json",
        channel: "terminal",
        direction: Direction::Input,
        title: "Octoboard terminal stream: client to daemon",
        description: "One text frame a client sends on `/ws/term/:session`; binary frames are raw \
                      terminal input. See `apps/daemon/PROTOCOL.md`.",
        root: |generator| generator.subschema_for::<TermControl>(),
    },
    Document {
        file: "terminal-output.schema.json",
        channel: "terminal",
        direction: Direction::Output,
        title: "Octoboard terminal stream: daemon to client",
        description:
            "One text frame the daemon sends on `/ws/term/:session`; binary frames are raw \
                      terminal output. See `apps/daemon/PROTOCOL.md`.",
        root: |generator| generator.subschema_for::<TermEvent>(),
    },
];

/// Every file's schema with its direction, for `crosscheck.rs`.
pub(super) fn rendered() -> Vec<(Direction, Value)> {
    rendered_files()
        .into_iter()
        .map(|(_, direction, schema)| (direction, schema))
        .collect()
}

/// Every file's name, direction and schema, for the compatibility guard.
pub(super) fn rendered_files() -> Vec<(&'static str, Direction, Value)> {
    DOCUMENTS
        .iter()
        .map(|document| (document.file, document.direction, render_value(document)))
        .collect()
}

/// One file's text: the root's keywords first, then the definitions ordered by name, so that the
/// rendering does not depend on the order types are reached in.
fn render(document: &Document) -> String {
    let text = serde_json::to_string_pretty(&render_value(document)).expect("a schema serialises");
    assert!(
        !text.contains(NULLABLE_MARK) && !text.contains(REQUIRED_MARK),
        "a field marker is left in {}: it was put on a schema that is not a property",
        document.file
    );
    format!("{text}\n")
}

fn render_value(document: &Document) -> Value {
    let settings = match document.direction {
        Direction::Input => SchemaSettings::draft2020_12().for_deserialize(),
        Direction::Output => SchemaSettings::draft2020_12().for_serialize(),
    };
    let mut generator = settings.into_generator();
    let mut root = (document.root)(&mut generator);
    let mut definitions: Vec<(String, Value)> =
        generator.take_definitions(false).into_iter().collect();
    definitions.sort_by(|(left, _), (right, _)| left.cmp(right));
    // schemars names a generic instance after its arguments and tells two types of the same name
    // apart with a number; neither is a name a client should be generated with.
    for (name, _) in &definitions {
        assert!(
            name.chars().all(|c| c.is_ascii_alphabetic()),
            "{} names a definition `{name}`: give the type a name of its own, or make its \
             schema inline",
            document.file
        );
    }
    WireRules.transform(&mut root);
    for (_, definition) in &mut definitions {
        let schema: &mut Schema = definition.try_into().expect("a definition is a schema");
        WireRules.transform(schema);
    }
    let direction = match document.direction {
        Direction::Input => "input",
        Direction::Output => "output",
    };
    let mut out = Map::new();
    out.insert(
        "$schema".to_string(),
        "https://json-schema.org/draft/2020-12/schema".into(),
    );
    out.insert(
        "$comment".to_string(),
        format!("Generated by `{REGENERATE}`; do not edit.").into(),
    );
    out.insert("title".to_string(), document.title.into());
    out.insert("description".to_string(), document.description.into());
    out.insert(
        "x-octoboard-protocol".to_string(),
        serde_json::json!({
            "generation": GENERATION,
            "channel": document.channel,
            "direction": direction,
        }),
    );
    let Value::Object(root) = root.to_value() else {
        panic!("a root schema is an object");
    };
    out.extend(root);
    out.insert(
        "$defs".to_string(),
        Value::Object(definitions.into_iter().collect()),
    );
    Value::Object(out)
}

#[test]
fn committed_schema_is_current() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(DIRECTORY);
    let write = std::env::var_os(super::bindings::WRITE_VARIABLE).is_some_and(|value| value == "1");
    let mut stale = Vec::new();
    for document in DOCUMENTS {
        let rendered = render(document);
        let path = directory.join(document.file);
        if write {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(&path, &rendered).unwrap();
            continue;
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        if committed != rendered {
            stale.push(format!(
                "contracts/protocol/{}:\n{}",
                document.file,
                first_difference(&committed, &rendered)
            ));
        }
    }
    let known: Vec<&str> = DOCUMENTS.iter().map(|document| document.file).collect();
    let unknown = unknown_schema_files(&directory, &known);
    assert!(
        unknown.is_empty(),
        "contracts/protocol/ holds files no wire type renders: {unknown:?}"
    );
    if !stale.is_empty() {
        panic!(
            "the committed schema is out of date with the wire types; regenerate it with \
             `{REGENERATE}` and commit it.\n{}",
            stale.join("\n")
        );
    }
}

/// The names of the schema files directly in `directory` that are not `known`: a snapshot kept
/// beside them in a directory of its own, a directory that happens to end in the suffix, or a file
/// the system leaves (`.DS_Store`), is not a rendering.
fn unknown_schema_files(directory: &std::path::Path, known: &[&str]) -> Vec<String> {
    let mut unknown: Vec<String> = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let name = entry.file_name().into_string().ok()?;
                    let is_file = entry.file_type().ok()?.is_file();
                    (is_file && name.ends_with(FILE_SUFFIX) && !known.contains(&name.as_str()))
                        .then_some(name)
                })
                .collect()
        })
        .unwrap_or_default();
    unknown.sort();
    unknown
}

/// The lines around the first one the committed file and the rendering differ in, numbered, `-`
/// for the committed file and `+` for the rendering.
fn first_difference(committed: &str, rendered: &str) -> String {
    const CONTEXT: usize = 3;
    let committed: Vec<&str> = committed.lines().collect();
    let rendered: Vec<&str> = rendered.lines().collect();
    let Some(first) = (0..committed.len().max(rendered.len()))
        .find(|&index| committed.get(index) != rendered.get(index))
    else {
        return "every line is the same: the line endings or the final line break differ".into();
    };
    let start = first.saturating_sub(CONTEXT);
    let side = |lines: &[&str], mark: char| {
        lines
            .iter()
            .enumerate()
            .skip(start)
            .take(first - start + CONTEXT + 1)
            .map(|(index, line)| format!("{mark} {:>5} {line}", index + 1))
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "first difference at line {}:\n{}\n{}",
        first + 1,
        side(&committed, '-'),
        side(&rendered, '+')
    )
}

/// `WireRules` applied to one object schema with the property `value`, listed as required or not.
fn after_rules(value: Value, required: bool) -> (Value, bool) {
    let mut schema = json_schema!({
        "type": "object",
        "properties": { "field": value },
        "required": if required { vec!["field"] } else { Vec::new() },
    });
    WireRules.transform(&mut schema);
    let value = schema.to_value();
    let listed = value["required"]
        .as_array()
        .is_some_and(|names| names.iter().any(|name| name == "field"));
    (value["properties"]["field"].clone(), listed)
}

#[test]
fn a_property_that_may_be_left_out_loses_null() {
    let cases = [
        (
            serde_json::json!({ "type": ["string", "null"] }),
            serde_json::json!({ "type": "string" }),
        ),
        (
            serde_json::json!({
                "anyOf": [{ "$ref": "#/$defs/A" }, { "type": "null" }],
                "description": "d",
            }),
            serde_json::json!({ "$ref": "#/$defs/A", "description": "d" }),
        ),
        (
            serde_json::json!({ "enum": ["a", null] }),
            serde_json::json!({ "const": "a" }),
        ),
    ];
    for (before, after) in cases {
        assert_eq!(after_rules(before, false), (after, false));
    }
}

#[test]
fn a_required_property_keeps_null() {
    let field = serde_json::json!({ "type": ["string", "null"] });
    assert_eq!(after_rules(field.clone(), true), (field, true));
}

#[test]
fn a_nullable_property_that_may_be_left_out_keeps_null() {
    let field = serde_json::json!({ "type": ["string", "null"], NULLABLE_MARK: true });
    let kept = serde_json::json!({ "type": ["string", "null"] });
    assert_eq!(after_rules(field, false), (kept, false));
}

#[test]
fn a_required_nullable_property_is_listed_as_required_and_keeps_null() {
    let field = serde_json::json!({ "type": ["string", "null"], REQUIRED_MARK: true });
    let kept = serde_json::json!({ "type": ["string", "null"] });
    assert_eq!(after_rules(field, false), (kept, true));
}

#[test]
fn the_rules_reach_the_properties_of_nested_objects() {
    let nested = serde_json::json!({
        "type": "object",
        "properties": { "inner": { "type": ["integer", "null"] } },
    });
    let (field, _) = after_rules(nested, true);
    assert_eq!(
        field["properties"]["inner"],
        serde_json::json!({ "type": "integer" })
    );
}

#[test]
fn the_files_are_the_four_channel_and_direction_pairs_in_draft_2020_12() {
    let files = rendered_files();
    let names: Vec<&str> = files.iter().map(|(file, _, _)| *file).collect();
    assert_eq!(
        names,
        [
            "control-input.schema.json",
            "control-output.schema.json",
            "terminal-input.schema.json",
            "terminal-output.schema.json",
        ]
    );
    for (file, direction, schema) in &files {
        assert_eq!(
            schema["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{file}"
        );
        let stated = &schema["x-octoboard-protocol"];
        assert_eq!(stated["generation"], GENERATION, "{file}");
        let channel = file.split('-').next().unwrap();
        let side = match direction {
            Direction::Input => "input",
            Direction::Output => "output",
        };
        assert_eq!(stated["channel"], channel, "{file}");
        assert_eq!(stated["direction"], side, "{file}");
        assert!(file.contains(side), "{file}");
    }
}

#[test]
fn a_file_that_differs_only_in_line_endings_says_so() {
    let report = first_difference("a\r\nb\r\n", "a\nb");
    assert!(report.contains("line endings"), "{report}");
}

#[test]
fn a_stale_file_is_reported_at_its_first_differing_line() {
    let report = first_difference("a\nb\nc\n", "a\nB\nc\n");
    assert!(
        report.starts_with("first difference at line 2:"),
        "{report}"
    );
    assert!(
        report.contains("-     2 b") && report.contains("+     2 B"),
        "{report}"
    );
}

#[test]
fn a_rendering_names_every_definition_it_reaches_and_ends_in_a_line_break() {
    for document in DOCUMENTS {
        let rendered = render(document);
        assert!(rendered.ends_with("}\n"), "{}", document.file);
        let value: Value = serde_json::from_str(&rendered).unwrap();
        let definitions = value["$defs"].as_object().unwrap();
        let mut named = std::collections::BTreeSet::new();
        collect_references(&value, &mut named);
        let defined: std::collections::BTreeSet<String> = definitions.keys().cloned().collect();
        assert_eq!(named, defined, "{}", document.file);
    }
}

/// The definition names of every `$ref` in `value`.
fn collect_references(value: &Value, names: &mut std::collections::BTreeSet<String>) {
    match value {
        Value::Object(object) => {
            for (key, inner) in object {
                match (key.as_str(), inner) {
                    ("$ref", Value::String(reference)) => {
                        let name = reference
                            .strip_prefix("#/$defs/")
                            .expect("a local reference");
                        names.insert(name.to_string());
                    }
                    _ => collect_references(inner, names),
                }
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|item| collect_references(item, names)),
        _ => {}
    }
}

#[test]
fn only_a_regular_unknown_schema_file_beside_the_renderings_is_reported() {
    let directory = crate::test_support::ScratchDir::new("unknown-schema-files");
    std::fs::write(directory.join("known.schema.json"), "{}").unwrap();
    std::fs::write(directory.join("unknown.schema.json"), "{}").unwrap();
    std::fs::create_dir(directory.join("released")).unwrap();
    std::fs::write(directory.join("released/other.schema.json"), "{}").unwrap();
    std::fs::create_dir(directory.join("x.schema.json")).unwrap();
    std::fs::write(directory.join(".DS_Store"), "").unwrap();
    assert_eq!(
        unknown_schema_files(&directory, &["known.schema.json"]),
        vec!["unknown.schema.json".to_string()]
    );
}
