//! Unit tests of the comparison and the guard, on small hand-written schemas.

use serde_json::{json, Map, Value};

use super::*;

/// A tagged object: `type` is the tag, `fields` the other properties, `required` the names
/// among them that may not be left out.
fn variant(tag: &str, fields: Value, required: &[&str]) -> Value {
    let mut properties = Map::new();
    properties.insert(
        "type".to_string(),
        json!({ "type": "string", "const": tag }),
    );
    properties.extend(fields.as_object().unwrap().clone());
    let mut names = vec!["type"];
    names.extend(required);
    json!({ "type": "object", "properties": properties, "required": names })
}

fn frame(variants: Vec<Value>) -> Value {
    json!({ "oneOf": variants })
}

/// A single object with one property, for the rules about one property's schema.
fn property(schema: Value, required: bool) -> Value {
    json!({
        "type": "object",
        "properties": { "field": schema },
        "required": if required { vec!["field"] } else { Vec::new() },
    })
}

fn failures(direction: Direction, old: &Value, new: &Value) -> Vec<String> {
    compare("fixture.schema.json", direction, old, new)
}

fn assert_holds(direction: Direction, old: Value, new: Value) {
    assert_eq!(failures(direction, &old, &new), Vec::<String>::new());
}

/// The comparison fails, naming the file, the path and the rule, and the way out.
fn assert_breaks(direction: Direction, old: Value, new: Value, path: &str, rule: &str) {
    let found = failures(direction, &old, &new);
    assert!(
        found.iter().any(|message| message
            .starts_with(&format!("fixture.schema.json {path}: {rule}"))
            && message.ends_with(WAY_OUT)),
        "no failure at {path} for `{rule}`: {found:#?}"
    );
}

const INPUT: Direction = Direction::Input;
const OUTPUT: Direction = Direction::Output;

#[test]
fn an_input_may_not_lose_a_request() {
    let old = frame(vec![
        variant("a", json!({}), &[]),
        variant("b", json!({}), &[]),
    ]);
    let new = frame(vec![variant("a", json!({}), &[])]);
    assert_breaks(INPUT, old, new, "#[type=b]", "the variant is removed");
}

#[test]
fn an_input_may_gain_a_request() {
    let old = frame(vec![variant("a", json!({}), &[])]);
    let new = frame(vec![
        variant("b", json!({}), &[]),
        variant("a", json!({}), &[]),
    ]);
    assert_holds(INPUT, old, new);
}

#[test]
fn an_input_may_not_lose_a_field() {
    let old = variant("a", json!({ "x": { "type": "string" } }), &[]);
    let new = variant("a", json!({}), &[]);
    assert_breaks(
        INPUT,
        old,
        new,
        "#[type=a]/properties/x",
        "the property is removed",
    );
}

#[test]
fn an_input_may_gain_an_optional_field() {
    let old = variant("a", json!({}), &[]);
    let new = variant("a", json!({ "x": { "type": "string" } }), &[]);
    assert_holds(INPUT, old, new);
}

#[test]
fn an_input_may_not_gain_a_required_field() {
    let old = variant("a", json!({}), &[]);
    let new = variant("a", json!({ "x": { "type": "string" } }), &["x"]);
    assert_breaks(
        INPUT,
        old,
        new,
        "#[type=a]/properties/x",
        "the property is newly required",
    );
}

#[test]
fn an_input_may_not_require_a_field_that_was_optional() {
    let old = variant("a", json!({ "x": { "type": "string" } }), &[]);
    let new = variant("a", json!({ "x": { "type": "string" } }), &["x"]);
    assert_breaks(
        INPUT,
        old,
        new,
        "#[type=a]/properties/x",
        "the property is newly required",
    );
}

#[test]
fn an_input_may_stop_requiring_a_field() {
    let old = variant("a", json!({ "x": { "type": "string" } }), &["x"]);
    let new = variant("a", json!({ "x": { "type": "string" } }), &[]);
    assert_holds(INPUT, old, new);
}

#[test]
fn an_input_may_not_lose_null() {
    let old = property(json!({ "type": ["string", "null"] }), false);
    let new = property(json!({ "type": "string" }), false);
    assert_breaks(
        INPUT,
        old,
        new,
        "#/properties/field",
        "no longer accepts null",
    );
}

#[test]
fn an_input_may_gain_null_in_any_of_its_spellings() {
    let old = property(json!({ "type": "string" }), false);
    for new in [
        json!({ "type": ["string", "null"] }),
        json!({ "anyOf": [{ "type": "string" }, { "type": "null" }] }),
    ] {
        assert_holds(INPUT, old.clone(), property(new, false));
    }
}

#[test]
fn null_is_the_same_whichever_way_it_is_spelled() {
    let spellings = [
        json!({ "type": ["string", "null"] }),
        json!({ "anyOf": [{ "type": "string" }, { "type": "null" }] }),
        json!({ "type": ["string", "null"], "enum": ["a", null] }),
    ];
    for old in &spellings[..2] {
        for new in &spellings[..2] {
            assert_holds(
                INPUT,
                property(old.clone(), false),
                property(new.clone(), false),
            );
            assert_holds(
                OUTPUT,
                property(old.clone(), false),
                property(new.clone(), false),
            );
        }
    }
}

#[test]
fn an_input_may_not_narrow_its_enum() {
    let old = property(json!({ "type": "string", "enum": ["a", "b"] }), true);
    let new = property(json!({ "type": "string", "enum": ["a"] }), true);
    assert_breaks(
        INPUT,
        old,
        new,
        "#/properties/field",
        "no longer accepts the values [\"b\"]",
    );
}

#[test]
fn an_input_may_grow_its_enum() {
    let old = property(json!({ "type": "string", "enum": ["a"] }), true);
    let new = property(json!({ "type": "string", "enum": ["a", "b"] }), true);
    assert_holds(INPUT, old, new);
}

#[test]
fn an_input_may_not_limit_a_string_to_an_enum() {
    let old = property(json!({ "type": "string" }), true);
    let new = property(json!({ "type": "string", "enum": ["a"] }), true);
    assert_breaks(INPUT, old, new, "#/properties/field", "accepted any string");
}

#[test]
fn an_input_may_not_lose_a_type() {
    let old = property(
        json!({ "anyOf": [{ "type": "boolean" }, { "type": "string" }] }),
        true,
    );
    let new = property(json!({ "type": "string" }), true);
    assert_breaks(
        INPUT,
        old,
        new,
        "#/properties/field",
        "no longer accepts a boolean",
    );
}

#[test]
fn an_input_may_gain_a_type() {
    let old = property(json!({ "type": "string" }), true);
    let new = property(
        json!({ "anyOf": [{ "type": "boolean" }, { "type": "string" }] }),
        true,
    );
    assert_holds(INPUT, old, new);
}

#[test]
fn an_input_may_not_narrow_an_integer_range_but_may_widen_it() {
    let wide = property(json!({ "type": "integer", "format": "int64" }), true);
    let narrow = property(
        json!({ "type": "integer", "format": "uint16", "minimum": 0 }),
        true,
    );
    assert_breaks(
        INPUT,
        wide.clone(),
        narrow.clone(),
        "#/properties/field",
        "accepted integers",
    );
    assert_holds(INPUT, narrow, wide);
    let bounded = property(
        json!({ "type": "integer", "minimum": 1, "maximum": 5 }),
        true,
    );
    let bigger = property(
        json!({ "type": "integer", "minimum": 1, "maximum": 6 }),
        true,
    );
    assert_breaks(
        INPUT,
        bigger.clone(),
        bounded.clone(),
        "#/properties/field",
        "accepted integers",
    );
    assert_holds(INPUT, bounded, bigger);
}

#[test]
fn an_input_may_not_lose_the_values_of_a_map() {
    let old = property(
        json!({ "type": "object", "additionalProperties": { "type": ["string", "null"] } }),
        true,
    );
    let new = property(
        json!({ "type": "object", "additionalProperties": { "type": "string" } }),
        true,
    );
    assert_breaks(
        INPUT,
        old,
        new,
        "#/properties/field/additionalProperties",
        "no longer accepts null",
    );
}

#[test]
fn an_input_may_not_narrow_the_keys_of_a_map() {
    let keys = |names: &[&str]| {
        property(
            json!({
                "type": "object",
                "propertyNames": { "type": "string", "enum": names },
                "additionalProperties": { "type": "string" },
            }),
            true,
        )
    };
    assert_breaks(
        INPUT,
        keys(&["a", "b"]),
        keys(&["a"]),
        "#/properties/field/propertyNames",
        "no longer accepts the values",
    );
    assert_holds(INPUT, keys(&["a"]), keys(&["a", "b"]));
}

#[test]
fn an_input_follows_references() {
    let schema = |extra: Value, tags: &[&str]| {
        json!({
            "oneOf": [{ "$ref": "#/$defs/Body" }],
            "$defs": { "Body": { "oneOf": tags.iter().map(|tag| variant(tag, extra.clone(), &[])).collect::<Vec<_>>() } },
        })
    };
    let old = schema(json!({}), &["a", "b"]);
    let new = schema(json!({}), &["a"]);
    assert_breaks(INPUT, old, new, "#[type=b]", "the variant is removed");
}

#[test]
fn a_recursive_definition_is_compared_once() {
    let node = |extra: &str| {
        json!({
            "$ref": "#/$defs/Node",
            "$defs": { "Node": { "type": "object", "properties": {
                "children": { "type": "array", "items": { "$ref": "#/$defs/Node" } },
                extra: { "type": "string" },
            } } },
        })
    };
    assert_holds(INPUT, node("a"), node("a"));
    assert_breaks(
        INPUT,
        node("a"),
        node("b"),
        "#/$defs/Node/properties/a",
        "the property is removed",
    );
}

#[test]
fn a_type_that_contains_itself_is_compared_once_however_the_reference_is_spelled() {
    // An `Option<Box<Self>>` is an `anyOf` with `null`; a reference with siblings is not bare.
    let node = |extra: &str| {
        json!({
            "$ref": "#/$defs/Node",
            "$defs": { "Node": { "type": "object", "properties": {
                "next": { "anyOf": [{ "$ref": "#/$defs/Node" }, { "type": "null" }] },
                "previous": { "$ref": "#/$defs/Node", "type": "object", "description": "the one before" },
                extra: { "type": "string" },
            } } },
        })
    };
    for direction in [INPUT, OUTPUT] {
        assert_holds(direction, node("a"), node("a"));
    }
    assert_breaks(
        INPUT,
        node("a"),
        node("b"),
        "#/$defs/Node/properties/a",
        "the property is removed",
    );
}

#[test]
fn a_trial_that_fails_leaves_no_failure_behind_when_a_later_alternative_holds() {
    // Two levels of choice: the outer alternatives are tried in turn, and the inner union is
    // reordered, so its first trial fails before the second holds.
    let record = |name: &str| json!({ "type": "object", "properties": { name: { "type": "string" } }, "required": [name] });
    let inner = |names: [&str; 2]| json!({ "anyOf": [record(names[0]), record(names[1])] });
    let old = json!({
        "type": "object",
        "properties": { "field": inner(["a", "b"]), "x": { "type": "string" } },
        "required": ["field", "x"],
    });
    let without_x = json!({
        "type": "object",
        "properties": { "field": inner(["a", "b"]) },
        "required": ["field"],
    });
    let reordered = json!({
        "type": "object",
        "properties": { "field": inner(["b", "a"]), "x": { "type": "string" } },
        "required": ["field", "x"],
    });
    assert_holds(INPUT, old, json!({ "anyOf": [without_x, reordered] }));
}

#[test]
fn a_nested_tagged_enum_is_matched_by_each_inner_variant() {
    // An event tagged by `type` whose `state` is a tagged enum of its own.
    let event = |failed_error_required: bool, ok_has_error: bool| {
        let state = |name: &str, error: bool, required: bool| {
            let mut properties = json!({
                "type": { "type": "string", "const": "progress" },
                "state": { "type": "string", "const": name },
            });
            let mut names = vec!["type", "state"];
            if error {
                properties["error"] = json!({ "type": "string" });
                if required {
                    names.push("error");
                }
            }
            json!({ "type": "object", "properties": properties, "required": names })
        };
        json!({ "oneOf": [
            state("ok", ok_has_error, false),
            state("failed", true, failed_error_required),
        ] })
    };
    assert_holds(OUTPUT, event(true, false), event(true, false));
    assert_breaks(
        OUTPUT,
        event(true, false),
        event(false, true),
        "#[state=failed,type=progress]/properties/error",
        "a required property may now be left out",
    );
}

#[test]
fn an_output_variant_may_not_be_split_into_alternatives_that_do_not_all_keep_its_shape() {
    // `{type: foo, error}` becomes two `foo` variants: `failed` keeps `error` required and `ok`
    // drops it, and a client takes the `ok` one for the old variant.
    let old = variant("foo", json!({ "error": { "type": "string" } }), &["error"]);
    let state = |name: &str, fields: Value, required: &[&str]| {
        let mut object = variant("foo", fields, required);
        object["properties"]["state"] = json!({ "type": "string", "const": name });
        object["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("state"));
        object
    };
    let failed = state(
        "failed",
        json!({ "error": { "type": "string" } }),
        &["error"],
    );
    let broken = json!({ "oneOf": [state("ok", json!({}), &[]), failed.clone()] });
    assert_breaks(
        OUTPUT,
        old.clone(),
        broken,
        "#[type=foo]/properties/error",
        "the property is removed",
    );
    // The same split with every alternative keeping `error` holds.
    let kept = json!({ "oneOf": [
        state("ok", json!({ "error": { "type": "string" } }), &["error"]),
        failed,
    ] });
    assert_holds(OUTPUT, old, kept);
}

#[test]
fn an_optional_output_struct_may_not_become_an_enum_whose_alternatives_do_not_all_keep_its_shape() {
    // Scenario: `Option<Struct>`, an `anyOf` with `null`, becomes an untagged enum of which only
    // `A` keeps the shape.
    let shape = |name: &str| json!({ "type": "object", "properties": { name: { "type": "string" } }, "required": [name] });
    let old = property(json!({ "anyOf": [shape("a"), { "type": "null" }] }), true);
    let only_a = property(
        json!({ "anyOf": [shape("a"), shape("b"), { "type": "null" }] }),
        true,
    );
    assert_breaks(
        OUTPUT,
        old.clone(),
        only_a,
        "#/properties/field/properties/a",
        "the property is removed",
    );
    // An input of the same change holds: some new alternative accepts the old shape.
    let grown = property(
        json!({ "anyOf": [shape("a"), shape("b"), { "type": "null" }] }),
        true,
    );
    assert_holds(INPUT, old, grown);
}

#[test]
fn a_constant_that_is_not_a_string_or_null_fails_rather_than_widens() {
    let schema = property(json!({ "type": "boolean", "const": true }), true);
    assert_breaks(
        INPUT,
        schema.clone(),
        schema,
        "#/properties/field",
        "the constant true",
    );
}

#[test]
fn the_two_forms_of_a_unit_enum_are_one() {
    let plain = json!({ "type": "string", "enum": ["a", "b"] });
    let documented = json!({ "oneOf": [
        { "type": "string", "const": "a", "description": "the first" },
        { "type": "string", "const": "b", "description": "the second" },
    ] });
    for direction in [INPUT, OUTPUT] {
        assert_holds(
            direction,
            property(plain.clone(), true),
            property(documented.clone(), true),
        );
        assert_holds(
            direction,
            property(documented.clone(), true),
            property(plain.clone(), true),
        );
    }
    let shrunk = json!({ "oneOf": [{ "type": "string", "const": "a" }] });
    assert_breaks(
        INPUT,
        property(plain, true),
        property(shrunk, true),
        "#/properties/field",
        "no longer accepts the values [\"b\"]",
    );
}

#[test]
fn a_flattened_type_and_a_narrowing_enum_beside_a_reference_are_folded_in() {
    let schema = |values: &[&str]| {
        json!({
            "type": "object",
            "allOf": [{ "$ref": "#/$defs/Base" }],
            "properties": {
                "kind": { "$ref": "#/$defs/Kind", "enum": values },
            },
            "$defs": {
                "Base": { "type": "object", "properties": { "id": { "type": "string" } }, "required": ["id"] },
                "Kind": { "type": "string", "enum": ["a", "b", "c"] },
            },
        })
    };
    assert_holds(INPUT, schema(&["a", "b"]), schema(&["a", "b", "c"]));
    assert_breaks(
        INPUT,
        schema(&["a", "b"]),
        schema(&["a"]),
        "#/properties/kind",
        "no longer accepts the values [\"b\"]",
    );
    // The flattened property is the type's own.
    let without_base = json!({ "type": "object", "properties": {} });
    assert_breaks(
        INPUT,
        schema(&["a"]),
        without_base,
        "#/properties/id",
        "the property is removed",
    );
}

#[test]
fn an_untagged_union_holds_while_one_alternative_takes_each_old_one() {
    let target = |names: &[&str]| {
        json!({ "anyOf": names.iter().map(|name| json!({
            "type": "object",
            "properties": { *name: { "type": "string" } },
            "required": [name],
        })).collect::<Vec<_>>() })
    };
    assert_holds(
        INPUT,
        target(&["session_id"]),
        target(&["project_id", "session_id"]),
    );
    assert_breaks(
        INPUT,
        target(&["session_id", "project_id"]),
        target(&["session_id"]),
        "#/properties/project_id",
        "the property is removed",
    );
}

#[test]
fn an_output_may_not_lose_an_event() {
    let old = frame(vec![
        variant("a", json!({}), &[]),
        variant("b", json!({}), &[]),
    ]);
    let new = frame(vec![variant("b", json!({}), &[])]);
    assert_breaks(OUTPUT, old, new, "#[type=a]", "the variant is removed");
}

#[test]
fn an_output_may_gain_an_event() {
    let old = frame(vec![variant("a", json!({}), &[])]);
    let new = frame(vec![
        variant("a", json!({}), &[]),
        variant("b", json!({}), &[]),
    ]);
    assert_holds(OUTPUT, old, new);
}

#[test]
fn an_output_may_not_lose_a_field() {
    let old = variant("a", json!({ "x": { "type": "string" } }), &[]);
    let new = variant("a", json!({}), &[]);
    assert_breaks(
        OUTPUT,
        old,
        new,
        "#[type=a]/properties/x",
        "the property is removed",
    );
}

#[test]
fn an_output_may_gain_an_optional_or_a_required_field() {
    let old = variant("a", json!({}), &[]);
    let optional = variant("a", json!({ "x": { "type": "string" } }), &[]);
    let required = variant("a", json!({ "x": { "type": "string" } }), &["x"]);
    assert_holds(OUTPUT, old.clone(), optional);
    assert_holds(OUTPUT, old, required);
}

#[test]
fn an_output_may_not_make_a_required_field_optional() {
    let old = variant("a", json!({ "x": { "type": "string" } }), &["x"]);
    let new = variant("a", json!({ "x": { "type": "string" } }), &[]);
    assert_breaks(
        OUTPUT,
        old,
        new,
        "#[type=a]/properties/x",
        "a required property may now be left out",
    );
}

#[test]
fn an_output_may_make_an_optional_field_required() {
    let old = variant("a", json!({ "x": { "type": "string" } }), &[]);
    let new = variant("a", json!({ "x": { "type": "string" } }), &["x"]);
    assert_holds(OUTPUT, old, new);
}

#[test]
fn an_output_may_not_gain_null() {
    let old = property(json!({ "type": "string" }), true);
    let new = property(json!({ "type": ["string", "null"] }), true);
    assert_breaks(
        OUTPUT,
        old,
        new,
        "#/properties/field",
        "gains a new kind of value, null",
    );
}

#[test]
fn an_output_may_not_lose_a_type() {
    let old = property(json!({ "type": ["string", "null"] }), true);
    let new = property(json!({ "type": "string" }), true);
    assert_breaks(
        OUTPUT,
        old,
        new,
        "#/properties/field",
        "loses a type it gave, null",
    );
}

#[test]
fn an_output_may_not_gain_a_new_kind_of_value() {
    let old = property(json!({ "type": "string" }), true);
    let new = property(
        json!({ "anyOf": [{ "type": "string" }, { "type": "boolean" }] }),
        true,
    );
    assert_breaks(
        OUTPUT,
        old,
        new,
        "#/properties/field",
        "gains a new kind of value, a boolean",
    );
}

#[test]
fn an_output_may_not_change_a_type() {
    let old = property(json!({ "type": "string" }), true);
    let new = property(
        json!({ "type": "integer", "format": "uint32", "minimum": 0 }),
        true,
    );
    assert_breaks(
        OUTPUT,
        old.clone(),
        new.clone(),
        "#/properties/field",
        "loses a type it gave, a string",
    );
    assert_breaks(
        OUTPUT,
        old,
        new,
        "#/properties/field",
        "gains a new kind of value, an integer",
    );
}

#[test]
fn an_output_may_not_widen_an_integer_range_but_may_narrow_it() {
    let narrow = property(
        json!({ "type": "integer", "format": "uint16", "minimum": 0 }),
        true,
    );
    let wide = property(
        json!({ "type": "integer", "format": "uint64", "minimum": 0 }),
        true,
    );
    assert_breaks(
        OUTPUT,
        narrow.clone(),
        wide.clone(),
        "#/properties/field",
        "gave integers",
    );
    assert_holds(OUTPUT, wide, narrow);
}

#[test]
fn an_output_enum_may_grow_or_shrink() {
    let old = property(json!({ "type": "string", "enum": ["a", "b"] }), true);
    let grown = property(json!({ "type": "string", "enum": ["a", "b", "c"] }), true);
    let shrunk = property(json!({ "type": "string", "enum": ["a"] }), true);
    assert_holds(OUTPUT, old.clone(), grown);
    assert_holds(OUTPUT, old, shrunk);
}

#[test]
fn an_output_may_not_lose_the_values_of_a_map_but_may_gain_keys() {
    let map = |values: Value, names: &[&str]| {
        property(
            json!({
                "type": "object",
                "propertyNames": { "type": "string", "enum": names },
                "additionalProperties": values,
            }),
            true,
        )
    };
    assert_holds(
        OUTPUT,
        map(json!({ "type": "string" }), &["a"]),
        map(json!({ "type": "string" }), &["a", "b"]),
    );
    assert_breaks(
        OUTPUT,
        map(json!({ "type": ["string", "null"] }), &["a"]),
        map(json!({ "type": "string" }), &["a"]),
        "#/properties/field/additionalProperties",
        "loses a type it gave, null",
    );
}

#[test]
fn an_output_array_keeps_the_kind_of_its_items() {
    let items = |schema: Value| property(json!({ "type": "array", "items": schema }), true);
    assert_breaks(
        OUTPUT,
        items(json!({ "type": "string" })),
        items(json!({ "type": "integer" })),
        "#/properties/field/items",
        "gains a new kind of value, an integer",
    );
    assert_holds(
        OUTPUT,
        items(json!({ "type": "string" })),
        items(json!({ "type": "string" })),
    );
}

#[test]
fn tagged_variants_are_matched_by_their_tag_not_by_position() {
    let old = frame(vec![
        variant("a", json!({ "x": { "type": "string" } }), &[]),
        variant("b", json!({ "y": { "type": "string" } }), &[]),
    ]);
    // The same variants in the other order, `b` having lost its field.
    let new = frame(vec![
        variant("b", json!({}), &[]),
        variant("a", json!({ "x": { "type": "string" } }), &[]),
    ]);
    for direction in [INPUT, OUTPUT] {
        assert_holds(direction, old.clone(), {
            frame(vec![
                variant("b", json!({ "y": { "type": "string" } }), &[]),
                variant("a", json!({ "x": { "type": "string" } }), &[]),
            ])
        });
        assert_breaks(
            direction,
            old.clone(),
            new.clone(),
            "#[type=b]/properties/y",
            "the property is removed",
        );
    }
}

#[test]
fn an_input_may_not_lose_the_null_of_the_items_of_an_array() {
    let items = |schema: Value| property(json!({ "type": "array", "items": schema }), true);
    assert_breaks(
        INPUT,
        items(json!({ "type": ["string", "null"] })),
        items(json!({ "type": "string" })),
        "#/properties/field/items",
        "no longer accepts null",
    );
    assert_holds(
        INPUT,
        items(json!({ "type": "string" })),
        items(json!({ "type": ["string", "null"] })),
    );
}

#[test]
fn an_input_map_may_stop_constraining_its_values() {
    let map = |values: Value| {
        property(
            json!({ "type": "object", "additionalProperties": values }),
            true,
        )
    };
    assert_holds(INPUT, map(json!({ "type": "string" })), map(json!(true)));
    let found = failures(INPUT, &map(json!(true)), &map(json!({ "type": "string" })));
    assert!(
        found.iter().any(
            |message| message.starts_with("fixture.schema.json #/properties/field")
                && message.ends_with(WAY_OUT)
        ),
        "{found:#?}"
    );
}

#[test]
fn an_output_map_may_not_stop_constraining_its_values() {
    let map = |values: Value| {
        property(
            json!({ "type": "object", "additionalProperties": values }),
            true,
        )
    };
    let found = failures(OUTPUT, &map(json!({ "type": "string" })), &map(json!(true)));
    assert!(
        found.iter().any(
            |message| message.starts_with("fixture.schema.json #/properties/field")
                && message.ends_with(WAY_OUT)
        ),
        "{found:#?}"
    );
    assert_holds(
        OUTPUT,
        map(json!({ "type": "string" })),
        map(json!({ "type": "string" })),
    );
}

#[test]
fn a_keyword_the_guard_does_not_read_fails_rather_than_passes() {
    let schema = property(json!({ "type": "string", "pattern": "^a" }), true);
    assert_breaks(
        INPUT,
        schema.clone(),
        schema,
        "#/properties/field",
        "the keyword `pattern`",
    );
}

#[test]
fn a_description_is_not_compared() {
    let old = property(json!({ "type": "string", "description": "one" }), true);
    let new = property(
        json!({ "type": "string", "description": "two", "title": "T" }),
        true,
    );
    assert_holds(INPUT, old.clone(), new.clone());
    assert_holds(OUTPUT, old, new);
}

/// An object tagged by `type` and a `state`, both required string constants.
fn stated(tag: &str, state: &str, fields: Value, required: &[&str]) -> Value {
    let mut object = variant(tag, fields, required);
    object["properties"]["state"] = json!({ "type": "string", "const": state });
    object["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("state"));
    object
}

/// An untagged object with one required string property.
fn record(name: &str) -> Value {
    json!({ "type": "object", "properties": { name: { "type": "string" } }, "required": [name] })
}

#[test]
fn the_tag_of_a_variant_is_all_of_its_required_string_constants() {
    let old = frame(vec![
        stated("p", "ok", json!({ "x": { "type": "string" } }), &[]),
        stated("p", "failed", json!({}), &[]),
    ]);
    // `p`/`ok` is gone although another `p` variant is left: the tag is the pair.
    let lost = frame(vec![stated("p", "failed", json!({}), &[])]);
    // `p`/`new` is a variant no old one is the tag of.
    let gained = frame(vec![
        stated("p", "ok", json!({ "x": { "type": "string" } }), &[]),
        stated("p", "failed", json!({}), &[]),
        stated("p", "new", json!({}), &[]),
    ]);
    for direction in [INPUT, OUTPUT] {
        assert_breaks(
            direction,
            old.clone(),
            lost.clone(),
            "#[state=ok,type=p]",
            "the variant is removed",
        );
    }
    assert_holds(OUTPUT, old.clone(), gained.clone());
    assert_holds(INPUT, old, gained);
}

#[test]
fn a_constant_that_is_not_required_is_not_part_of_the_tag() {
    let optional_kind = |fields: Value| {
        json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "const": "a" },
                "x": fields,
            },
            "required": ["x"],
        })
    };
    let old = optional_kind(json!({ "type": "string" }));
    // Were `kind` a tag, the second alternative would carry no old tag and be a new variant.
    let new = json!({ "anyOf": [old.clone(), record("y")] });
    assert_breaks(
        OUTPUT,
        old,
        new,
        "#/properties/x",
        "the property is removed",
    );
}

#[test]
fn an_input_nested_tagged_enum_may_not_lose_an_inner_variant() {
    let event = |states: &[&str]| json!({ "oneOf": states.iter().map(|state| stated("progress", state, json!({}), &[])).collect::<Vec<_>>() });
    assert_holds(
        INPUT,
        event(&["ok", "failed"]),
        event(&["failed", "ok", "new"]),
    );
    assert_breaks(
        INPUT,
        event(&["ok", "failed"]),
        event(&["ok"]),
        "#[state=failed,type=progress]",
        "the variant is removed",
    );
}

#[test]
fn an_untagged_output_union_may_not_gain_an_alternative_unless_it_keeps_an_old_shape() {
    let union = |alternatives: Vec<Value>| json!({ "anyOf": alternatives });
    let old = union(vec![record("a")]);
    assert_breaks(
        OUTPUT,
        old.clone(),
        union(vec![record("a"), record("b")]),
        "#/properties/a",
        "the property is removed",
    );
    // An added alternative that still has the old shape (and more) is taken for it safely.
    let richer = json!({
        "type": "object",
        "properties": { "a": { "type": "string" }, "b": { "type": "string" } },
        "required": ["a"],
    });
    assert_holds(OUTPUT, old, union(vec![record("a"), richer]));
}

#[test]
fn an_untagged_output_alternative_is_kept_when_one_new_alternative_holds_for_it() {
    // Two old shapes, one new alternative that keeps the first: a client takes it for the first.
    let old = json!({ "anyOf": [record("a"), record("b")] });
    assert_holds(OUTPUT, old.clone(), json!({ "anyOf": [record("a")] }));
    assert_breaks(
        OUTPUT,
        old,
        json!({ "anyOf": [record("c")] }),
        "#/properties/a",
        "the property is removed",
    );
}

#[test]
fn a_constant_the_guard_does_not_read_fails_in_an_enum_and_in_either_direction() {
    for schema in [
        json!({ "enum": [1, 2] }),
        json!({ "enum": ["a", 1] }),
        json!({ "const": 7 }),
        json!({ "const": ["a"] }),
    ] {
        for direction in [INPUT, OUTPUT] {
            let found = failures(direction, &schema, &schema);
            assert!(
                found.iter().any(|message| message.contains("the constant")),
                "{schema} {direction:?}: {found:#?}"
            );
        }
    }
    for schema in [json!({ "enum": ["a", null] }), json!({ "const": null })] {
        for direction in [INPUT, OUTPUT] {
            assert_holds(direction, schema.clone(), schema.clone());
        }
    }
}

#[test]
fn a_recursive_output_definition_reports_instead_of_overflowing() {
    let node = |extra: &str| {
        json!({
            "$ref": "#/$defs/Node",
            "$defs": { "Node": { "type": "object", "properties": {
                "next": { "anyOf": [{ "$ref": "#/$defs/Node" }, { "type": "null" }] },
                extra: { "type": "string" },
            } } },
        })
    };
    assert_holds(OUTPUT, node("a"), node("a"));
    assert_breaks(
        OUTPUT,
        node("a"),
        node("b"),
        "#/$defs/Node/properties/a",
        "the property is removed",
    );
}

#[test]
fn two_definitions_that_refer_to_each_other_terminate_in_either_direction() {
    let pair = |extra: &str| {
        json!({
            "$ref": "#/$defs/A",
            "$defs": {
                "A": { "type": "object", "properties": { "b": { "$ref": "#/$defs/B" }, extra: { "type": "string" } } },
                "B": { "type": "object", "properties": { "a": { "anyOf": [{ "$ref": "#/$defs/A" }, { "type": "null" }] } } },
            },
        })
    };
    for direction in [INPUT, OUTPUT] {
        assert_holds(direction, pair("x"), pair("x"));
        let found = failures(direction, &pair("x"), &pair("y"));
        assert!(
            found
                .iter()
                .any(|message| message.contains("the property is removed")),
            "{found:#?}"
        );
    }
}

/// A snapshot of one file, as the release records it.
fn released(generation: u32, files: &[(&str, &Value)]) -> Released {
    Released {
        record: Record {
            version: "0.0.1".to_string(),
            generation,
        },
        files: files
            .iter()
            .map(|(name, schema)| (name.to_string(), (*schema).clone()))
            .collect(),
    }
}

fn broken() -> (Value, Value) {
    (
        variant("a", json!({ "x": { "type": "string" } }), &[]),
        variant("a", json!({}), &[]),
    )
}

#[test]
fn a_lower_generation_than_the_last_release_fails() {
    let (old, new) = broken();
    let found = check(
        2,
        &[("f.schema.json", INPUT, new)],
        &released(3, &[("f.schema.json", &old)]),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("never lowered"), "{found:?}");
}

#[test]
fn a_raised_generation_allows_a_break_and_compares_nothing() {
    let (old, new) = broken();
    let found = check(
        4,
        &[("f.schema.json", INPUT, new)],
        &released(3, &[("f.schema.json", &old)]),
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_same_generation_compares_each_file_with_its_snapshot() {
    let (old, new) = broken();
    let snapshot = released(3, &[("f.schema.json", &old)]);
    let found = check(3, &[("f.schema.json", INPUT, new)], &snapshot);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("f.schema.json #[type=a]/properties/x:"),
        "{found:?}"
    );
    assert_eq!(
        check(3, &[("f.schema.json", INPUT, old)], &snapshot),
        Vec::<String>::new()
    );
}

#[test]
fn the_same_generation_without_the_snapshot_files_fails() {
    let current = json!({ "type": "object" });
    let found = check(3, &[("f.schema.json", INPUT, current)], &released(3, &[]));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("holds no schema files") && found[0].contains("GENERATION"),
        "{found:?}"
    );
}

#[test]
fn the_same_generation_fails_on_a_snapshot_file_the_schema_no_longer_renders() {
    let kept = json!({ "type": "object" });
    let gone = json!({ "type": "object" });
    let found = check(
        3,
        &[("kept.schema.json", INPUT, kept.clone())],
        &released(
            3,
            &[("kept.schema.json", &kept), ("gone.schema.json", &gone)],
        ),
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("gone.schema.json: the file is removed")
            && found[0].ends_with(WAY_OUT),
        "{found:?}"
    );
}

#[test]
fn every_broken_file_is_reported() {
    let (old, new) = broken();
    let snapshot = released(3, &[("f.schema.json", &old), ("g.schema.json", &old)]);
    let found = check(
        3,
        &[
            ("f.schema.json", INPUT, new.clone()),
            ("g.schema.json", OUTPUT, new),
            ("h.schema.json", INPUT, old),
        ],
        &snapshot,
    );
    assert!(
        found
            .iter()
            .any(|message| message.starts_with("f.schema.json "))
            && found
                .iter()
                .any(|message| message.starts_with("g.schema.json "))
            && found
                .iter()
                .all(|message| !message.starts_with("h.schema.json")),
        "{found:#?}"
    );
}

#[test]
fn the_same_generation_passes_a_file_the_snapshot_does_not_have() {
    let kept = json!({ "type": "object" });
    let added = json!({ "type": "object" });
    let found = check(
        3,
        &[
            ("kept.schema.json", INPUT, kept.clone()),
            ("added.schema.json", OUTPUT, added),
        ],
        &released(3, &[("kept.schema.json", &kept)]),
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_generation_no_release_carried_may_change_in_place_without_files() {
    let current = json!({ "type": "object" });
    let found = check(3, &[("f.schema.json", INPUT, current)], &released(2, &[]));
    assert_eq!(found, Vec::<String>::new());
}
