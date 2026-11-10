//! The check that the two renderings of the wire types agree: for every property of every named
//! type, whether it may be left out and whether it may be `null` are the same in the TypeScript
//! bindings (`bindings.rs`) as in the schema (`schema.rs`). Both are derived from the same Rust
//! types, but each reads its own attributes, so a field can say one thing to one and another to the
//! other; this is what keeps the web client's types checked against the schema.
//!
//! Each declaration `ts-rs` writes is parsed into its object types, and each object type of a
//! schema definition is paired with one of them by its tag (a property whose type is one string,
//! such as `"type": "hello"`), else by its property names. A type the bindings flatten into the
//! type that flattens it, where the schema keeps a definition of its own, is looked for in the
//! declarations of the types that name it with `allOf`.
//!
//! The bindings declare a type once, while the schema describes each direction on its own:
//! - a type only the client sends is compared with the input schema, exactly;
//! - a type the daemon sends, with the output schema, exactly: a skipped `Option` field needs
//!   `#[ts(optional)]`, as `ts-rs` would otherwise render it `?: T | null`, and a skipped
//!   `Option<Option<T>>`, which writes `null` for `Some(None)`, needs the schema's `nullable`;
//! - a type both send is compared with the output schema, exactly, and the input schema must accept
//!   what its declaration allows, since a client sends such a record back as it was given: a
//!   property optional there is optional in the input, and one nullable there is nullable in it.
//!
//! The differences that remain are listed in `ALLOWED`, each with its reason. Only properties are
//! compared, not the values of a map such as a `PerAgent`'s (see `schema.rs` for where they
//! differ).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use super::schema::Direction;

/// The properties whose optionality or nullability may differ between the input schema and the
/// bindings of a type only the client sends: the type's name, its tag value (empty for an object
/// without a tag), the property, and why.
const ALLOWED: &[(&str, &str, &str, &str)] = &[(
    "RequestBody",
    "read_account_mcp_inventory",
    "refresh",
    "a serde default lets the client leave it out; the bindings ask for it, as for a field with \
     no `ts` attribute",
)];

#[test]
fn the_bindings_and_the_schema_agree_on_every_property() {
    let declarations: BTreeMap<String, Vec<TsObject>> = super::bindings::declarations()
        .into_iter()
        .map(|(name, declaration)| {
            let objects = parse_declaration(&declaration)
                .unwrap_or_else(|error| panic!("cannot read the declaration of {name}: {error}"));
            (name, objects)
        })
        .collect();
    let documents = super::schema::rendered();
    // The control and terminal files of one direction share one namespace of type names: a name
    // both reach is the same type, so its definitions are the same.
    let definitions = |wanted: Direction| -> BTreeMap<String, Value> {
        let mut all: BTreeMap<String, Value> = BTreeMap::new();
        for (direction, document) in &documents {
            if *direction == wanted {
                let defs = document["$defs"]
                    .as_object()
                    .expect("a schema has definitions");
                for (name, definition) in defs {
                    if let Some(earlier) = all.insert(name.clone(), definition.clone()) {
                        assert_eq!(
                            &earlier, definition,
                            "two {wanted:?} files define {name} differently"
                        );
                    }
                }
            }
        }
        all
    };
    let input = definitions(Direction::Input);
    let output = definitions(Direction::Output);
    let mut problems = Vec::new();
    let mut allowed_used = BTreeSet::new();
    // The declarations objects were looked for in, and the objects of theirs that were paired.
    let mut searched: BTreeSet<&str> = BTreeSet::new();
    let mut paired: BTreeSet<*const TsObject> = BTreeSet::new();
    for (direction, defs, other) in [
        (Direction::Input, &input, &output),
        (Direction::Output, &output, &input),
    ] {
        for (name, definition) in defs {
            let rule = match (direction, other.contains_key(name)) {
                (Direction::Output, _) => Rule::Output,
                (Direction::Input, false) => Rule::Input,
                (Direction::Input, true) => Rule::Accepts,
            };
            let groups = candidates_for(name, defs, &declarations);
            if groups.is_empty() && !schema_objects(definition).is_empty() {
                problems.push(format!(
                    "{direction:?} {name}: no declaration of the bindings"
                ));
            }
            for (declaration, candidates) in &groups {
                searched.insert(declaration);
                for object in schema_objects(definition) {
                    let tags = schema_tags(object);
                    let Some(ts) = pair(object, &tags, candidates) else {
                        problems.push(format!(
                            "{direction:?} {name} {tags:?}: no single object of the bindings' \
                             {declaration} matches"
                        ));
                        continue;
                    };
                    paired.insert(std::ptr::from_ref(ts));
                    let tag = tags.values().next().cloned().unwrap_or_default();
                    compare(
                        (direction, name, &tag),
                        object,
                        ts,
                        rule,
                        &mut problems,
                        &mut allowed_used,
                    );
                }
            }
        }
    }
    for declaration in &searched {
        for object in &declarations[*declaration] {
            if !paired.contains(&std::ptr::from_ref(object)) {
                let names: Vec<&String> = object.fields.keys().collect();
                problems.push(format!(
                    "the bindings' {declaration} has an object no schema object matches: {names:?}"
                ));
            }
        }
    }
    for (name, tag, field, _) in ALLOWED {
        if !allowed_used.contains(&(name.to_string(), tag.to_string(), field.to_string())) {
            problems.push(format!(
                "ALLOWED lists {name} {tag:?} .{field}, which agrees now: take it off the list"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "the TypeScript bindings and the schema disagree; give the field the schema attribute that \
         matches its `ts` one (see `schema.rs`):\n{}",
        problems.join("\n")
    );
}

#[derive(Clone, Copy)]
enum Rule {
    /// A type only the client sends: the same in both.
    Input,
    /// A type the daemon sends, or the output view of one both send: the same in both.
    Output,
    /// The input view of a type both send: accepts at least what the declaration allows.
    Accepts,
}

/// Compares one schema object with its object of the bindings, property by property.
fn compare(
    (direction, name, tag): (Direction, &str, &str),
    schema: &Map<String, Value>,
    ts: &TsObject,
    rule: Rule,
    problems: &mut Vec<String>,
    allowed_used: &mut BTreeSet<(String, String, String)>,
) {
    let properties = schema["properties"]
        .as_object()
        .expect("an object schema has properties");
    let required: BTreeSet<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let place = format!("{direction:?} {name} {tag:?}");
    for field in ts.fields.keys() {
        if !properties.contains_key(field) {
            problems.push(format!(
                "{place} .{field}: in the bindings, not in the schema"
            ));
        }
    }
    for (field, property) in properties {
        let Some(ts_field) = ts.fields.get(field) else {
            problems.push(format!(
                "{place} .{field}: in the schema, not in the bindings"
            ));
            continue;
        };
        let schema_optional = !required.contains(field.as_str());
        let schema_nullable = is_nullable(property);
        let ts_nullable = ts_field.ty.is_nullable();
        // `unknown` and an open schema say nothing about `null`.
        let nullable_known = !ts_field.ty.is_unknown() && property != &Value::Bool(true);
        let agrees = match rule {
            Rule::Input | Rule::Output => {
                ts_field.optional == schema_optional
                    && (!nullable_known || ts_nullable == schema_nullable)
            }
            Rule::Accepts => {
                (!ts_field.optional || schema_optional)
                    && (!nullable_known || !ts_nullable || schema_nullable)
            }
        };
        if agrees {
            continue;
        }
        let key = (name.to_string(), tag.to_string(), field.clone());
        let allowed = matches!(rule, Rule::Input)
            && ALLOWED
                .iter()
                .any(|(n, t, f, _)| (*n, *t, *f) == (name, tag, field.as_str()));
        if allowed {
            allowed_used.insert(key);
            continue;
        }
        problems.push(format!(
            "{place} .{field}: the bindings say {}{}, the schema {}{}",
            if ts_field.optional {
                "optional"
            } else {
                "required"
            },
            if ts_nullable { " and nullable" } else { "" },
            if schema_optional {
                "optional"
            } else {
                "required"
            },
            if schema_nullable { " and nullable" } else { "" },
        ));
    }
}

/// Whether a property schema allows `null`, in any of the ways schemars writes it.
fn is_nullable(schema: &Value) -> bool {
    let is_null_type = |value: &Value| match value.get("type") {
        Some(Value::String(name)) => name == "null",
        Some(Value::Array(names)) => names.iter().any(|name| name == "null"),
        _ => false,
    };
    let enum_allows = schema
        .get("enum")
        .and_then(Value::as_array)
        .map(|values| values.contains(&Value::Null));
    if enum_allows == Some(false) {
        return false;
    }
    is_null_type(schema)
        || enum_allows == Some(true)
        || schema
            .get("anyOf")
            .and_then(Value::as_array)
            .is_some_and(|alternatives| alternatives.iter().any(is_null_type))
}

/// The object schemas of a definition: itself, or the alternatives it is made of. A `$ref` is not
/// followed, as the definition it names is checked on its own, but an alternative that names one
/// and adds properties of its own (an internally tagged newtype variant, with its tag) is an
/// object here.
fn schema_objects(definition: &Value) -> Vec<&Map<String, Value>> {
    let mut objects = Vec::new();
    fn visit<'a>(schema: &'a Value, objects: &mut Vec<&'a Map<String, Value>>) {
        let Some(object) = schema.as_object() else {
            return;
        };
        if object.contains_key("properties") {
            objects.push(object);
        }
        for keyword in ["oneOf", "anyOf", "allOf"] {
            for alternative in object
                .get(keyword)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                visit(alternative, objects);
            }
        }
    }
    visit(definition, &mut objects);
    objects
}

/// The properties whose type is one string: an enum's tag, or a constant.
fn schema_tags(object: &Map<String, Value>) -> BTreeMap<String, String> {
    object["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, property)| {
            Some((name.clone(), property.get("const")?.as_str()?.to_string()))
        })
        .collect()
}

/// The declarations of the bindings a definition's objects are looked for in, each with its
/// objects: its own, and those of every type that flattens it, directly or through another, since
/// `ts-rs` writes a flattened type's objects into the declaration of the type that flattens it.
fn candidates_for<'a>(
    name: &str,
    defs: &BTreeMap<String, Value>,
    declarations: &'a BTreeMap<String, Vec<TsObject>>,
) -> Vec<(&'a str, Vec<&'a TsObject>)> {
    let mut groups = Vec::new();
    let mut pending = vec![name.to_string()];
    let mut seen = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        if let Some((declared, objects)) = declarations.get_key_value(&current) {
            groups.push((declared.as_str(), objects.iter().collect()));
        }
        let reference = format!("#/$defs/{current}");
        for (other, definition) in defs {
            if names_with_all_of(definition, &reference) {
                pending.push(other.clone());
            }
        }
    }
    groups
}

fn names_with_all_of(schema: &Value, reference: &str) -> bool {
    match schema {
        Value::Object(object) => object.iter().any(|(key, value)| {
            (key == "allOf"
                && value
                    .as_array()
                    .is_some_and(|items| items.iter().any(|item| item["$ref"] == reference)))
                || names_with_all_of(value, reference)
        }),
        Value::Array(items) => items.iter().any(|item| names_with_all_of(item, reference)),
        _ => false,
    }
}

/// The one object of the bindings with the same tags, narrowed to the same property names when
/// the tags leave more than one.
fn pair<'a>(
    object: &Map<String, Value>,
    tags: &BTreeMap<String, String>,
    candidates: &[&'a TsObject],
) -> Option<&'a TsObject> {
    let tagged: Vec<&TsObject> = candidates
        .iter()
        .copied()
        .filter(|candidate| &candidate.tags() == tags)
        .collect();
    if let [only] = tagged.as_slice() {
        return Some(only);
    }
    let names: BTreeSet<&String> = object["properties"].as_object()?.keys().collect();
    let named: Vec<&TsObject> = tagged
        .into_iter()
        .filter(|candidate| candidate.fields.keys().collect::<BTreeSet<_>>() == names)
        .collect();
    match named.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

/// An object type of a declaration, by field name.
#[derive(Debug)]
struct TsObject {
    fields: BTreeMap<String, TsField>,
}

impl TsObject {
    fn tags(&self) -> BTreeMap<String, String> {
        self.fields
            .iter()
            .filter(|(_, field)| !field.optional)
            .filter_map(|(name, field)| match &field.ty {
                TsType::Literal(value) => Some((name.clone(), value.clone())),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug)]
struct TsField {
    optional: bool,
    ty: TsType,
}

/// A TypeScript type as `ts-rs` writes it, reduced to what the check reads.
#[derive(Debug)]
enum TsType {
    Object(TsObject),
    Union(Vec<TsType>),
    Intersection(Vec<TsType>),
    /// A string literal, without its quotes.
    Literal(String),
    /// `null`.
    Null,
    /// Any other named type, its arguments read past: `string`, `Array<T>`, `Agent`.
    Named(String),
    /// A tuple, an index signature's object, a number literal.
    Other,
}

impl TsType {
    fn is_nullable(&self) -> bool {
        match self {
            TsType::Null => true,
            TsType::Union(members) => members.iter().any(TsType::is_nullable),
            _ => false,
        }
    }

    fn is_unknown(&self) -> bool {
        matches!(self, TsType::Named(name) if name == "unknown" || name == "any")
    }

    /// The object types the type is made of, through unions and intersections.
    fn into_objects(self, objects: &mut Vec<TsObject>) {
        match self {
            TsType::Object(object) => objects.push(object),
            TsType::Union(members) | TsType::Intersection(members) => members
                .into_iter()
                .for_each(|member| member.into_objects(objects)),
            _ => {}
        }
    }
}

/// Reads `type Name<…> = …;` into the object types it declares.
fn parse_declaration(declaration: &str) -> Result<Vec<TsObject>, String> {
    let tokens = tokenize(declaration)?;
    let mut parser = Parser { tokens, at: 0 };
    parser.expect("type")?;
    parser.next_token()?;
    if parser.eat("<") {
        while !parser.eat(">") {
            parser.next_token()?;
        }
    }
    parser.expect("=")?;
    let ty = parser.parse_type()?;
    parser.eat(";");
    if parser.at != parser.tokens.len() {
        return Err(format!("unread input from token {}", parser.at));
    }
    let mut objects = Vec::new();
    ty.into_objects(&mut objects);
    Ok(objects)
}

/// Splits a declaration into tokens: names, quoted strings (kept with their quotes), numbers and
/// single punctuation characters. Comments are dropped.
fn tokenize(text: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '/' if chars.peek() == Some(&'*') => {
                let mut previous = ' ';
                loop {
                    let inner = chars.next().ok_or("an unclosed comment")?;
                    if previous == '*' && inner == '/' {
                        break;
                    }
                    previous = inner;
                }
            }
            '"' => {
                let mut literal = String::from('"');
                loop {
                    let inner = chars.next().ok_or("an unclosed string")?;
                    literal.push(inner);
                    if inner == '\\' {
                        literal.push(chars.next().ok_or("an unclosed string")?);
                    } else if inner == '"' {
                        break;
                    }
                }
                tokens.push(literal);
            }
            c if c.is_alphanumeric() || c == '_' || c == '$' => {
                let mut word = String::from(c);
                while let Some(&next) = chars.peek() {
                    if next.is_alphanumeric() || next == '_' || next == '$' || next == '.' {
                        word.push(next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(word);
            }
            '{' | '}' | '(' | ')' | '[' | ']' | '<' | '>' | ',' | ':' | '?' | '|' | '&' | '='
            | ';' | '-' => tokens.push(c.to_string()),
            other => return Err(format!("an unexpected character {other:?}")),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<String>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }

    fn next_token(&mut self) -> Result<String, String> {
        let token = self.tokens.get(self.at).cloned().ok_or("an early end")?;
        self.at += 1;
        Ok(token)
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.peek() == Some(token) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &str) -> Result<(), String> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(format!(
                "expected {token:?} at token {}, found {:?}",
                self.at,
                self.peek()
            ))
        }
    }

    fn parse_type(&mut self) -> Result<TsType, String> {
        self.eat("|");
        let mut members = vec![self.parse_intersection()?];
        while self.eat("|") {
            members.push(self.parse_intersection()?);
        }
        Ok(if members.len() == 1 {
            members.remove(0)
        } else {
            TsType::Union(members)
        })
    }

    fn parse_intersection(&mut self) -> Result<TsType, String> {
        let mut members = vec![self.parse_primary()?];
        while self.eat("&") {
            members.push(self.parse_primary()?);
        }
        Ok(if members.len() == 1 {
            members.remove(0)
        } else {
            TsType::Intersection(members)
        })
    }

    fn parse_primary(&mut self) -> Result<TsType, String> {
        let token = self.next_token()?;
        let ty =
            match token.as_str() {
                "(" => {
                    let inner = self.parse_type()?;
                    self.expect(")")?;
                    inner
                }
                "{" => self.parse_object()?,
                "[" => {
                    while !self.eat("]") {
                        self.parse_type()?;
                        self.eat(",");
                    }
                    TsType::Other
                }
                "-" => {
                    self.next_token()?;
                    TsType::Other
                }
                "null" => TsType::Null,
                literal if literal.starts_with('"') => {
                    let inner = &literal[1..literal.len() - 1];
                    TsType::Literal(serde_json::from_str(&format!("\"{inner}\"")).map_err(
                        |error| format!("an unreadable string literal {literal}: {error}"),
                    )?)
                }
                number if number.starts_with(|c: char| c.is_ascii_digit()) => TsType::Other,
                name => {
                    if self.eat("<") {
                        loop {
                            self.parse_type()?;
                            if self.eat(">") {
                                break;
                            }
                            self.expect(",")?;
                        }
                    }
                    TsType::Named(name.to_string())
                }
            };
        if self.peek() == Some("[") && self.tokens.get(self.at + 1).map(String::as_str) == Some("]")
        {
            self.at += 2;
            return Ok(TsType::Named("Array".to_string()));
        }
        Ok(ty)
    }

    /// The members after `{`, up to and with the closing `}`. An index signature
    /// (`[key in string]: T`) makes the object a map, which the check does not compare.
    fn parse_object(&mut self) -> Result<TsType, String> {
        let mut fields = BTreeMap::new();
        let mut is_map = false;
        while !self.eat("}") {
            if self.eat("[") {
                while !self.eat("]") {
                    self.next_token()?;
                }
                is_map = true;
            } else {
                let name = self.next_token()?;
                let name = name.trim_matches('"').to_string();
                let optional = self.eat("?");
                self.expect(":")?;
                let ty = self.parse_type()?;
                fields.insert(name, TsField { optional, ty });
                self.eat(",");
                continue;
            }
            self.eat("?");
            self.expect(":")?;
            self.parse_type()?;
            self.eat(",");
        }
        Ok(if is_map {
            TsType::Other
        } else {
            TsType::Object(TsObject { fields })
        })
    }
}

#[test]
fn a_declaration_is_read_into_its_objects_with_their_optional_and_nullable_fields() {
    let objects = parse_declaration(
        "type A = { id?: string, } & ({ \"type\": \"x\", /** Says what b is. */ \
         b: Array<B> | null, c?: { [key in string]: number }, } \
         | { \"type\": \"y\", d: [string, number], });",
    )
    .unwrap();
    assert_eq!(objects.len(), 3);
    let id = &objects[0].fields["id"];
    assert!(id.optional && !id.ty.is_nullable());
    assert_eq!(
        objects[1].tags(),
        BTreeMap::from([("type".into(), "x".into())])
    );
    let b = &objects[1].fields["b"];
    assert!(!b.optional && b.ty.is_nullable());
    assert!(objects[1].fields["c"].optional);
    assert_eq!(
        objects[2].tags(),
        BTreeMap::from([("type".into(), "y".into())])
    );
}

#[test]
fn a_property_is_nullable_in_each_way_schemars_writes_it() {
    let nullable = [
        serde_json::json!({ "type": ["string", "null"] }),
        serde_json::json!({ "anyOf": [{ "$ref": "#/$defs/A" }, { "type": "null" }] }),
        serde_json::json!({ "enum": ["a", null] }),
    ];
    for schema in nullable {
        assert!(is_nullable(&schema), "{schema}");
    }
    let narrowed = serde_json::json!({
        "anyOf": [{ "$ref": "#/$defs/A" }, { "type": "null" }],
        "enum": ["a"],
    });
    assert!(!is_nullable(&narrowed));
    assert!(!is_nullable(&serde_json::json!({ "type": "string" })));
}

/// The `#[ts(type = …)]` and `#[ts(as = …)]` overrides whose schema the derive already gets right,
/// so their field needs no `schemars` attribute beside them: the TypeScript says no more than the
/// Rust type does, or than what the field reads, for the direction it is read in. Each is the
/// override's keyword, its type as written in the attribute, and why.
const TS_OVERRIDES_THE_DERIVE_MATCHES: &[(&str, &str, &str)] = &[
    (
        "type",
        "Record<string, number>",
        "a map of `u64`, which the schema gives as integers and `ts-rs` would give as `bigint`",
    ),
    (
        "type",
        "Array<Agent>",
        "a set of agent ids, an array of strings, `Agent` being a string",
    ),
    (
        "type",
        "Record<Agent, T>",
        "`PerAgent` itself, whose schema `schema.rs` writes by hand",
    ),
    (
        "type",
        "unknown",
        "a `serde_json::Value`, which the schema leaves open",
    ),
    (
        "type",
        "Partial<Record<Agent, string>>",
        "a map keyed by agent id, `Agent` being a string; `Partial` adds nothing in JSON Schema",
    ),
    (
        "type",
        "Partial<Record<Agent, Array<string>>>",
        "as `Partial<Record<Agent, string>>`",
    ),
    (
        "type",
        "Partial<Record<Agent, Array<string | null>>>",
        "as `Partial<Record<Agent, string>>`",
    ),
    (
        "type",
        "Partial<Record<Agent, Record<string, boolean | string>>>",
        "as `Partial<Record<Agent, string>>`",
    ),
    (
        "type",
        "Partial<Record<Agent, Record<string, boolean | string | null>>>",
        "as `Partial<Record<Agent, string>>`",
    ),
    (
        "as",
        "Option<BTreeMap<String, Option<bool>>>",
        "a map with a serde default, which the schema also lets a request leave out, its values \
         `bool` or `null` in both",
    ),
    (
        "as",
        "bindings::PartialPerAgent<bindings::LaunchRequest>",
        "a `PerAgent<Launch>` read from a request, whose input schema is `LaunchRequest`'s, and \
         a `PerAgent`'s schema has no required agent",
    ),
    (
        "as",
        "bindings::PartialPerAgent<Change<String>>",
        "a `PerAgent<Change<String>>`, the same schema, a `PerAgent`'s having no required agent",
    ),
    (
        "as",
        "bindings::PartialPerAgent<Change<ProjectAccount>>",
        "as `bindings::PartialPerAgent<Change<String>>`",
    ),
    (
        "as",
        "bindings::PartialPerAgent<LaunchChange>",
        "a `PerAgent<LaunchChange>`, the same schema, a `PerAgent`'s having no required agent",
    ),
    (
        "as",
        "bindings::PartialPerAgent<String>",
        "a `PerAgent<Option<String>>`, whose schema also allows `null` values, as its description \
         says (see `schema.rs`); the cross-check compares properties, not map values",
    ),
    (
        "as",
        "bindings::PartialPerAgent<ProjectAccount>",
        "as `bindings::PartialPerAgent<String>`",
    ),
];

/// Whether a source file derives `TS`, spelled as the repository's other scans spell it: `TS` as
/// a word inside any `derive(...)`, whatever path it is named by.
fn derives_ts(text: &str) -> bool {
    // Spelled in two parts so that this file, which only names it, is not taken for one.
    let derive = concat!("der", "ive(");
    text.match_indices(derive).any(|(start, _)| {
        let list = &text[start + derive.len()..];
        let list = &list[..list.find(')').unwrap_or(list.len())];
        list.split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "TS")
    })
}

/// A `ts` attribute that changes what the bindings say of a field, `type = …`, `as = …` or
/// `optional = nullable` on an `Option`, needs the schema to say the same; the cross-check above
/// catches most such slips by comparing the renderings, and this catches the ones it cannot see,
/// a narrowed type among them. It reads every source file that derives `TS`.
#[test]
fn every_ts_attribute_that_changes_a_field_has_its_schema_counterpart() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    let mut pending = vec![root.join("src")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && derives_ts(&std::fs::read_to_string(&path).unwrap())
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut problems = Vec::new();
    let mut matched = BTreeSet::new();
    let mut seen = 0;
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap();
        let name = file.strip_prefix(&root).unwrap().display().to_string();
        for (line, attribute, field) in ts_attributes(&text) {
            let compact = without_spaces(&attribute);
            let overrides: Vec<(&str, String)> = ["type", "as"]
                .into_iter()
                .filter_map(|keyword| Some((keyword, keyword_value(&compact, keyword)?)))
                .collect();
            let nullable = has_keyword(&compact, "optional=nullable");
            if overrides.is_empty() && !nullable {
                continue;
            }
            seen += 1;
            if compact.contains("schemars(") {
                continue;
            }
            if !overrides.is_empty() {
                let all_matched = overrides.iter().all(|(keyword, ty)| {
                    let known = TS_OVERRIDES_THE_DERIVE_MATCHES
                        .iter()
                        .find(|(k, t, _)| k == keyword && t == ty);
                    if let Some((k, t, _)) = known {
                        matched.insert((*k, *t));
                    }
                    known.is_some()
                });
                if all_matched {
                    continue;
                }
            } else {
                // `ts-rs` adds no `null` to a field that is not an `Option`: it is only `?:`.
                let ty = field.split_once(':').map_or("", |(_, ty)| ty.trim());
                if !ty.starts_with("Option<") && !ty.starts_with("Present<") {
                    continue;
                }
            }
            problems.push(format!("{name}:{line}: {attribute}"));
        }
    }
    assert!(
        seen > 50,
        "the scan found only {seen} attributes; it no longer reads them"
    );
    for (keyword, ty, _) in TS_OVERRIDES_THE_DERIVE_MATCHES {
        if !matched.contains(&(*keyword, *ty)) {
            problems.push(format!(
                "TS_OVERRIDES_THE_DERIVE_MATCHES lists {keyword} {ty:?}, which no field uses"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "these `ts` attributes change a field for the bindings with nothing beside them for the \
         schema; add the matching `schemars` attribute in the same `cfg_attr` (see `schema.rs`), \
         or list an override the derive already matches in TS_OVERRIDES_THE_DERIVE_MATCHES:\n{}",
        problems.join("\n")
    );
}

/// Every `#[cfg_attr(…)]` holding a `ts(…)` in `text`: its line, its text, and the field it is on
/// (the rest of its line, else the next line that is not an attribute or a comment).
fn ts_attributes(text: &str) -> Vec<(usize, String, String)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = text[from..].find("#[cfg_attr(") {
        let start = from + offset;
        let end = attribute_end(text, start);
        from = end;
        let attribute = &text[start..end];
        if !attribute.contains("ts(") {
            continue;
        }
        let line = text[..start].matches('\n').count() + 1;
        let attribute: String = attribute.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut rest = text[end..].lines();
        let same_line = rest.next().unwrap_or("").trim();
        let field = if same_line.is_empty() {
            let mut in_attribute = false;
            rest.map(str::trim)
                .find(|line| {
                    if in_attribute {
                        in_attribute = !line.ends_with(']');
                        return false;
                    }
                    if line.starts_with("#[") {
                        in_attribute = !line.ends_with(']');
                        return false;
                    }
                    !line.is_empty() && !line.starts_with("//")
                })
                .unwrap_or("")
        } else {
            same_line
        };
        found.push((line, attribute, field.to_string()));
    }
    found
}

/// `text` without the whitespace outside its string literals.
fn without_spaces(text: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in text.chars() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
        } else if c == '"' {
            in_string = true;
        } else if c.is_whitespace() {
            continue;
        }
        out.push(c);
    }
    out
}

/// Whether `compact` (see [`without_spaces`]) holds `pattern` starting a word.
fn has_keyword(compact: &str, pattern: &str) -> bool {
    compact.match_indices(pattern).any(|(index, _)| {
        !compact[..index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

/// The string given to `keyword =` in `compact`, as written between its quotes.
fn keyword_value(compact: &str, keyword: &str) -> Option<String> {
    let pattern = format!("{keyword}=\"");
    let (index, _) = compact.match_indices(&pattern).find(|(index, _)| {
        !compact[..*index]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
    })?;
    let rest = &compact[index + pattern.len()..];
    let mut escaped = false;
    for (offset, c) in rest.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(rest[..offset].to_string()),
            _ => {}
        }
    }
    None
}

#[test]
fn an_attribute_keyword_is_found_whatever_its_spacing() {
    let compact = without_spaces("ts(optional  =\n nullable, as = \"A<B, C>\", rename = \"x y\")");
    assert!(has_keyword(&compact, "optional=nullable"));
    assert_eq!(keyword_value(&compact, "as").as_deref(), Some("A<B, C>"));
    assert_eq!(keyword_value(&compact, "type"), None);
    assert!(keyword_value("ts(subtype=\"x\")", "type").is_none());
}

/// The index just past the `]` that closes the attribute starting at `start`.
fn attribute_end(text: &str, start: usize) -> usize {
    let mut depth = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (index, c) in text[start..].char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '[' | '(' => depth += 1,
            ']' | ')' => {
                depth -= 1;
                if depth == 0 {
                    return start + index + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}
