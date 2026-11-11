use serde_json::{json, Value};

use super::kinds::KINDS;
use super::*;

fn parse(page: Value) -> Result<PageContent, PageContentError> {
    PageContent::parse(&page)
}

/// A body of `components` with a `submit` after them, so that a form is a form.
fn form(components: Vec<Value>) -> Value {
    let mut body = components;
    body.push(json!({ "type": "submit" }));
    json!({ "body": body })
}

/// A page of Markdown, a section holding a form, and the buttons that send it is accepted, and
/// reads back as it was written.
#[test]
fn a_page_of_markdown_sections_and_a_form_is_accepted() {
    let page = json!({
        "title": "Release plan",
        "body": [
            { "type": "markdown", "text": "# Plan\n\n| a | b |\n|---|---|\n| 1 | 2 |" },
            {
                "type": "section",
                "heading": "Your answers",
                "children": [
                    { "type": "text", "name": "who", "label": "Name", "required": true,
                      "pattern": "[A-Z].*", "min_length": 1, "max_length": 40 },
                    { "type": "checkboxes", "name": "topics", "label": "Topics",
                      "options": [{ "value": "rust" }, { "value": "sql", "label": "SQL" }],
                      "default": ["rust"], "min_selected": 1 },
                    { "type": "toggle", "name": "more", "label": "More?" },
                    { "type": "textarea", "name": "why", "label": "Why", "rows": 4,
                      "visible_when": { "field": "more", "equals": true } },
                ],
            },
            { "type": "choices", "name": "action", "label": "Decide",
              "options": [{ "value": "approve" }, { "value": "reject", "label": "Reject" }] },
        ],
    });
    let content = parse(page.clone()).expect("a valid page");
    assert_eq!(content.title.as_deref(), Some("Release plan"));
    assert_eq!(content.body.len(), 3);
    // Nothing is added to the tree or taken from it: an absent property stays absent.
    assert_eq!(serde_json::to_value(&content).unwrap(), page);
}

/// A page with no input fields is a plain report, and needs no submitter.
#[test]
fn a_page_without_inputs_needs_no_submitter() {
    parse(json!({ "body": [{ "type": "markdown", "text": "done" }] })).expect("a report");
}

/// Each way a tree can be wrong is refused with the JSON path of the failing part, the kind whose
/// schema explains it (none: the page object's), and a message that says what is wrong.
#[test]
fn an_invalid_tree_is_refused_with_its_path_and_kind() {
    let text = |extra: Value| {
        let mut field = json!({ "type": "text", "name": "t", "label": "T" });
        field
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        form(vec![field])
    };
    let choice = |kind: &str, extra: Value| {
        let mut field = json!({
            "type": kind, "name": "c", "label": "C",
            "options": [{ "value": "a" }, { "value": "b" }],
        });
        field
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        form(vec![field])
    };
    let cases: Vec<(Value, &str, Option<&str>, &str)> = vec![
        (json!({}), "", None, "missing required property `body`"),
        (json!({ "body": [] }), "body", None, "at least 1"),
        (
            json!({ "body": "x" }),
            "body",
            None,
            "must be array, not string",
        ),
        (
            json!({ "title": " ", "body": [{ "type": "markdown", "text": "x" }] }),
            "title",
            None,
            "blank",
        ),
        (
            json!({ "html": "<p>", "body": [{ "type": "markdown", "text": "x" }] }),
            "html",
            None,
            "unknown property",
        ),
        (
            json!({ "body": [3] }),
            "body[0]",
            None,
            "must be object, not number",
        ),
        (json!({ "body": [{}] }), "body[0]", None, "missing `type`"),
        (
            json!({ "body": [{ "type": "video" }] }),
            "body[0].type",
            None,
            "not a kind of component",
        ),
        (
            json!({ "body": [{ "type": "section", "children": [{ "type": "markdown" }] }] }),
            "body[0].children[0]",
            Some("markdown"),
            "missing required property `text`",
        ),
        (
            json!({ "body": [{ "type": "section", "children": [] }] }),
            "body[0].children",
            Some("section"),
            "at least 1",
        ),
        (
            json!({ "body": [{ "type": "section", "children": [4] }] }),
            "body[0].children[0]",
            Some("section"),
            "must be object, not number",
        ),
        (
            json!({ "body": [{ "type": "markdown", "text": "x", "color": "red" }] }),
            "body[0].color",
            Some("markdown"),
            "unknown property",
        ),
        (
            json!({ "body": [{ "type": "markdown", "text": 3 }] }),
            "body[0].text",
            Some("markdown"),
            "must be string, not number",
        ),
        (
            json!({ "body": [{ "type": "markdown", "text": "x", "visible_when": { "field": "f" } }] }),
            "body[0].visible_when",
            Some("markdown"),
            "missing required property `equals`",
        ),
        (
            text(json!({ "required": "yes" })),
            "body[0].required",
            Some("text"),
            "must be boolean",
        ),
        (
            text(json!({ "min_length": -1 })),
            "body[0].min_length",
            Some("text"),
            "at least 0",
        ),
        (
            text(json!({ "max_length": 0 })),
            "body[0].max_length",
            Some("text"),
            "at least 1",
        ),
        (
            text(json!({ "label": "" })),
            "body[0].label",
            Some("text"),
            "blank",
        ),
        (
            form(vec![
                json!({ "type": "textarea", "name": "t", "label": "T", "rows": 41 }),
            ]),
            "body[0].rows",
            Some("textarea"),
            "at most 40",
        ),
        (
            form(vec![
                json!({ "type": "number", "name": "n", "label": "N", "step": 0 }),
            ]),
            "body[0].step",
            Some("number"),
            "greater than 0",
        ),
        (
            form(vec![
                json!({ "type": "toggle", "name": "n", "label": "N", "required": true }),
            ]),
            "body[0].required",
            Some("toggle"),
            "unknown property",
        ),
        (
            choice("radio", json!({ "options": [{ "value": "a" }, {}] })),
            "body[0].options[1]",
            Some("radio"),
            "missing required property `value`",
        ),
        (
            choice("select", json!({ "options": [] })),
            "body[0].options",
            Some("select"),
            "at least 1",
        ),
        // What a schema cannot state.
        (
            text(json!({ "min_length": 5, "max_length": 2 })),
            "body[0].min_length",
            Some("text"),
            "above",
        ),
        (
            text(json!({ "default": "abc", "max_length": 2 })),
            "body[0].default",
            Some("text"),
            "length",
        ),
        (
            form(vec![
                json!({ "type": "number", "name": "n", "label": "N", "min": 5, "max": 1 }),
            ]),
            "body[0].min",
            Some("number"),
            "above `max`",
        ),
        (
            form(vec![
                json!({ "type": "number", "name": "n", "label": "N", "min": 1, "default": 0 }),
            ]),
            "body[0].default",
            Some("number"),
            "outside",
        ),
        (
            form(vec![
                json!({ "type": "date", "name": "d", "label": "D", "default": "2025-02-30" }),
            ]),
            "body[0].default",
            Some("date"),
            "not a valid YYYY-MM-DD",
        ),
        (
            form(vec![
                json!({ "type": "time", "name": "d", "label": "D", "min": "24:00" }),
            ]),
            "body[0].min",
            Some("time"),
            "not a valid HH:MM",
        ),
        (
            form(vec![json!({ "type": "datetime", "name": "d", "label": "D",
                              "min": "2025-01-02T10:00", "max": "2025-01-01T10:00" })]),
            "body[0].min",
            Some("datetime"),
            "after `max`",
        ),
        (
            form(vec![json!({ "type": "date", "name": "d", "label": "D",
                              "min": "2025-01-02", "default": "2025-01-01" })]),
            "body[0].default",
            Some("date"),
            "outside",
        ),
        (
            choice(
                "radio",
                json!({ "options": [{ "value": "a" }, { "value": "a" }] }),
            ),
            "body[0].options[1].value",
            Some("radio"),
            "earlier option",
        ),
        (
            choice("select", json!({ "default": "z" })),
            "body[0].default",
            Some("select"),
            "not the value",
        ),
        (
            choice("checkboxes", json!({ "default": ["a", "z"] })),
            "body[0].default[1]",
            Some("checkboxes"),
            "not the value",
        ),
        (
            choice("checkboxes", json!({ "default": ["a", "a"] })),
            "body[0].default[1]",
            Some("checkboxes"),
            "twice",
        ),
        (
            choice("checkboxes", json!({ "min_selected": 3 })),
            "body[0].min_selected",
            Some("checkboxes"),
            "more than the 2 options",
        ),
        (
            choice(
                "checkboxes",
                json!({ "min_selected": 2, "max_selected": 1 }),
            ),
            "body[0].min_selected",
            Some("checkboxes"),
            "above `max_selected`",
        ),
        (
            form(vec![
                json!({ "type": "slider", "name": "s", "label": "S", "min": 1, "max": 1 }),
            ]),
            "body[0].min",
            Some("slider"),
            "below `max`",
        ),
        (
            form(vec![
                json!({ "type": "slider", "name": "s", "label": "S", "min": 1, "max": 3, "default": 4 }),
            ]),
            "body[0].default",
            Some("slider"),
            "outside",
        ),
        (
            form(vec![
                json!({ "type": "text", "name": "a", "label": "A" }),
                json!({ "type": "section", "children": [{ "type": "toggle", "name": "a", "label": "A" }] }),
            ]),
            "body[1].children[0].name",
            Some("toggle"),
            "already the name of the input field at `body[0]`",
        ),
        (
            form(vec![
                json!({ "type": "text", "name": "a", "label": "A" }),
                json!({ "type": "submit", "name": "a", "value": "x" }),
            ]),
            "body[1].name",
            Some("submit"),
            "also the name of the input field at `body[0]`",
        ),
        (
            json!({ "body": [
                { "type": "choices", "name": "go", "options": [{ "value": "a" }] },
                { "type": "choices", "name": "go", "options": [{ "value": "b" }] },
            ] }),
            "body[1].name",
            Some("choices"),
            "already the name of the submitter",
        ),
        (
            json!({ "body": [{ "type": "submit", "name": "go" }] }),
            "body[0]",
            Some("submit"),
            "go together",
        ),
        (
            json!({ "body": [{ "type": "text", "name": "a", "label": "A" }] }),
            "body",
            None,
            "needs a `submit` or `choices`",
        ),
        (
            form(vec![json!({ "type": "markdown", "text": "x",
                              "visible_when": { "field": "ghost", "equals": "y" } })]),
            "body[0].visible_when.field",
            Some("markdown"),
            "no input field named `ghost`",
        ),
        (
            form(vec![json!({ "type": "text", "name": "a", "label": "A",
                              "visible_when": { "field": "a", "equals": "y" } })]),
            "body[0].visible_when.field",
            Some("text"),
            "own field",
        ),
        (
            form(vec![
                json!({ "type": "section", "visible_when": { "field": "a", "equals": "y" },
                              "children": [{ "type": "text", "name": "a", "label": "A" }] }),
            ]),
            "body[0].visible_when.field",
            Some("section"),
            "own field",
        ),
        (
            form(vec![
                json!({ "type": "columns", "visible_when": { "field": "a", "equals": true },
                              "children": [
                                  { "type": "markdown", "text": "x" },
                                  { "type": "section", "children": [
                                      { "type": "toggle", "name": "a", "label": "A" },
                                  ] },
                              ] }),
            ]),
            "body[0].visible_when.field",
            Some("columns"),
            "own field",
        ),
        (
            form(vec![
                json!({ "type": "toggle", "name": "a", "label": "A" }),
                json!({ "type": "markdown", "text": "x",
                        "visible_when": { "field": "a", "equals": "yes" } }),
            ]),
            "body[1].visible_when.equals",
            Some("markdown"),
            "holds a boolean",
        ),
        (
            form(vec![
                json!({ "type": "radio", "name": "a", "label": "A", "options": [{ "value": "x" }] }),
                json!({ "type": "markdown", "text": "x",
                        "visible_when": { "field": "a", "equals": "nope" } }),
            ]),
            "body[1].visible_when.equals",
            Some("markdown"),
            "not the value of any option",
        ),
        // Report components.
        (
            json!({ "body": [{ "type": "columns", "children": [{ "type": "markdown", "text": "x" }] }] }),
            "body[0].children",
            Some("columns"),
            "at least 2",
        ),
        (
            json!({ "body": [{ "type": "columns", "children": [{ "type": "markdown", "text": "x" }, { "type": "markdown", "text": "x" }, { "type": "markdown", "text": "x" }, { "type": "markdown", "text": "x" }, { "type": "markdown", "text": "x" }] }] }),
            "body[0].children",
            Some("columns"),
            "4",
        ),
        (
            json!({ "body": [{ "type": "columns", "children": [{ "type": "markdown", "text": "x" }, 7] }] }),
            "body[0].children[1]",
            Some("columns"),
            "must be object, not number",
        ),
        (
            json!({ "body": [{ "type": "columns", "children": [{ "type": "markdown", "text": "x" }, { "type": "markdown", "text": "x" }], "gap": 2 }] }),
            "body[0].gap",
            Some("columns"),
            "unknown property",
        ),
        (
            json!({ "body": [{ "type": "tabs", "tabs": [{ "label": "A", "children": [{ "type": "markdown", "text": "x" }] }] }] }),
            "body[0].tabs",
            Some("tabs"),
            "at least 2",
        ),
        (
            json!({ "body": [{ "type": "tabs", "tabs": [
                { "label": "A", "children": [{ "type": "markdown", "text": "x" }] }, { "label": "B", "children": [] },
            ] }] }),
            "body[0].tabs[1].children",
            Some("tabs"),
            "at least 1",
        ),
        (
            json!({ "body": [{ "type": "tabs", "tabs": [
                { "label": " ", "children": [{ "type": "markdown", "text": "x" }] }, { "label": "B", "children": [{ "type": "markdown", "text": "x" }] },
            ] }] }),
            "body[0].tabs[0].label",
            Some("tabs"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "tabs", "tabs": [
                { "label": "A", "children": [{ "type": "markdown", "text": "x" }] }, { "children": [{ "type": "markdown", "text": "x" }] },
            ] }] }),
            "body[0].tabs[1]",
            Some("tabs"),
            "missing required property `label`",
        ),
        (
            json!({ "body": [{ "type": "tabs", "tabs": [
                { "label": "A", "children": [{ "type": "markdown", "text": "x" }], "icon": "x" }, { "label": "B", "children": [{ "type": "markdown", "text": "x" }] },
            ] }] }),
            "body[0].tabs[0].icon",
            Some("tabs"),
            "unknown property",
        ),
        (
            json!({ "body": [{ "type": "tabs", "tabs": [
                { "label": "A", "children": [{ "type": "markdown", "text": "x" }] },
                { "label": "B", "children": [
                    { "type": "markdown", "text": "x", "visible_when": { "field": "ghost", "equals": "y" } },
                ] },
            ] }] }),
            "body[0].tabs[1].children[0].visible_when.field",
            Some("markdown"),
            "no input field named `ghost`",
        ),
        (
            json!({ "body": [{ "type": "divider", "label": " " }] }),
            "body[0].label",
            Some("divider"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "divider", "tone": "loud" }] }),
            "body[0].tone",
            Some("divider"),
            "unknown property",
        ),
        (
            json!({ "body": [{ "type": "callout" }] }),
            "body[0]",
            Some("callout"),
            "missing required property `text`",
        ),
        (
            json!({ "body": [{ "type": "callout", "text": " " }] }),
            "body[0].text",
            Some("callout"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "callout", "text": "x", "title": "" }] }),
            "body[0].title",
            Some("callout"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "callout", "text": "x", "tone": "loud" }] }),
            "body[0].tone",
            Some("callout"),
            "must be one of",
        ),
        (
            json!({ "body": [{ "type": "callout", "text": "x", "icon": "bell" }] }),
            "body[0].icon",
            Some("callout"),
            "unknown property",
        ),
        (
            json!({ "body": [{ "type": "key_values", "items": [] }] }),
            "body[0].items",
            Some("key_values"),
            "at least 1",
        ),
        (
            json!({ "body": [{ "type": "key_values", "items": [{ "key": "a" }] }] }),
            "body[0].items[0]",
            Some("key_values"),
            "missing required property `value`",
        ),
        (
            json!({ "body": [{ "type": "key_values", "items": [{ "key": " ", "value": 1 }] }] }),
            "body[0].items[0].key",
            Some("key_values"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "key_values", "items": [{ "key": "a", "value": null }] }] }),
            "body[0].items[0].value",
            Some("key_values"),
            "not null",
        ),
        (
            json!({ "body": [{ "type": "key_values", "items": [{ "key": "a", "value": { "x": 1 } }] }] }),
            "body[0].items[0].value",
            Some("key_values"),
            "not object",
        ),
        (
            json!({ "body": [{ "type": "key_values", "items": [{ "key": "a", "value": [1] }] }] }),
            "body[0].items[0].value",
            Some("key_values"),
            "not array",
        ),
        (
            json!({ "body": [{ "type": "stat", "value": 1 }] }),
            "body[0]",
            Some("stat"),
            "missing required property `label`",
        ),
        (
            json!({ "body": [{ "type": "stat", "label": "L" }] }),
            "body[0]",
            Some("stat"),
            "missing required property `value`",
        ),
        (
            json!({ "body": [{ "type": "stat", "label": "L", "value": true }] }),
            "body[0].value",
            Some("stat"),
            "not boolean",
        ),
        (
            json!({ "body": [{ "type": "stat", "label": " ", "value": 1 }] }),
            "body[0].label",
            Some("stat"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "stat", "label": "L", "value": 1, "change": "" }] }),
            "body[0].change",
            Some("stat"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "stat", "label": "L", "value": 1, "trend": "sideways" }] }),
            "body[0].trend",
            Some("stat"),
            "must be one of",
        ),
        (
            json!({ "body": [{ "type": "stat", "label": "L", "value": 1, "sentiment": "good" }] }),
            "body[0].sentiment",
            Some("stat"),
            "must be one of",
        ),
        (
            json!({ "body": [{ "type": "code" }] }),
            "body[0]",
            Some("code"),
            "missing required property `code`",
        ),
        (
            json!({ "body": [{ "type": "code", "code": " " }] }),
            "body[0].code",
            Some("code"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "code", "code": "x", "theme": "dark" }] }),
            "body[0].theme",
            Some("code"),
            "unknown property",
        ),
        (
            json!({ "body": [{ "type": "diff" }] }),
            "body[0]",
            Some("diff"),
            "missing required property `diff`",
        ),
        (
            json!({ "body": [{ "type": "diff", "diff": "" }] }),
            "body[0].diff",
            Some("diff"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "diff", "diff": "-a\n+b", "label": 3 }] }),
            "body[0].label",
            Some("diff"),
            "must be string, not number",
        ),
        (
            json!({ "body": [{ "type": "copyable" }] }),
            "body[0]",
            Some("copyable"),
            "missing required property `text`",
        ),
        (
            json!({ "body": [{ "type": "copyable", "text": "" }] }),
            "body[0].text",
            Some("copyable"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "copyable", "text": "x", "label": " " }] }),
            "body[0].label",
            Some("copyable"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "copyable", "text": "x", "monospace": "yes" }] }),
            "body[0].monospace",
            Some("copyable"),
            "must be boolean",
        ),
        (
            json!({ "body": [{ "type": "table", "rows": [] }] }),
            "body[0]",
            Some("table"),
            "missing required property `columns`",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A" }] }] }),
            "body[0]",
            Some("table"),
            "missing required property `rows`",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [], "rows": [] }] }),
            "body[0].columns",
            Some("table"),
            "at least 1",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": " " }], "rows": [] }] }),
            "body[0].columns[0].label",
            Some("table"),
            "blank",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A", "align": "center" }],
                              "rows": [] }] }),
            "body[0].columns[0].align",
            Some("table"),
            "must be one of",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A" }], "rows": ["x"] }] }),
            "body[0].rows[0]",
            Some("table"),
            "must be array, not string",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A" }],
                              "rows": [[{ "x": 1 }]] }] }),
            "body[0].rows[0][0]",
            Some("table"),
            "not object",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A" }],
                              "rows": [[[1]]] }] }),
            "body[0].rows[0][0]",
            Some("table"),
            "not array",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A" }], "rows": [],
                              "sortable": "yes" }] }),
            "body[0].sortable",
            Some("table"),
            "must be boolean",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A" }], "rows": [],
                              "filterable": 1 }] }),
            "body[0].filterable",
            Some("table"),
            "must be boolean",
        ),
        (
            json!({ "body": [{ "type": "table", "columns": [{ "label": "A" }], "rows": [],
                              "caption": " " }] }),
            "body[0].caption",
            Some("table"),
            "blank",
        ),
        // `visible_when` on a report component names an input field of the page.
        (
            form(vec![json!({ "type": "stat", "label": "L", "value": 1,
                              "visible_when": { "field": "ghost", "equals": "y" } })]),
            "body[0].visible_when.field",
            Some("stat"),
            "no input field named `ghost`",
        ),
    ];
    for (page, path, kind, message) in cases {
        let error = parse(page.clone()).expect_err(&format!("{page}"));
        assert_eq!(error.path, path, "{page}: {error}");
        assert_eq!(error.kind, kind, "{page}: {error}");
        assert!(error.message.contains(message), "{page}: {error}");
        assert!(error.to_string().contains(path), "{error}");
        // The schema handed back is that of the kind the failure is in, or of the page.
        assert_eq!(
            error.schema(),
            schema_named(kind.unwrap_or("page")).unwrap(),
            "{page}"
        );
    }
}

/// One component of each kind with every property it takes, which a page of it must accept as it
/// is: the typed form and the schema of a kind agree on what the kind has.
fn full_examples() -> Vec<(&'static str, Value)> {
    let when = json!({ "field": "other", "equals": "x" });
    let options = json!([{ "value": "a", "label": "A" }, { "value": "b" }]);
    vec![
        (
            "section",
            json!({ "type": "section", "heading": "H", "visible_when": when,
            "children": [{ "type": "markdown", "text": "x" }] }),
        ),
        (
            "markdown",
            json!({ "type": "markdown", "text": "x", "visible_when": when }),
        ),
        (
            "text",
            json!({ "type": "text", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "default": "ab", "placeholder": "p",
            "pattern": "[a-z]+", "min_length": 1, "max_length": 5 }),
        ),
        (
            "textarea",
            json!({ "type": "textarea", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "default": "ab", "placeholder": "p",
            "min_length": 1, "max_length": 5, "rows": 3 }),
        ),
        (
            "number",
            json!({ "type": "number", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "default": 2, "min": 1, "max": 3.5,
            "step": 0.5 }),
        ),
        (
            "date",
            json!({ "type": "date", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "default": "2024-02-29", "min": "2024-01-01",
            "max": "2024-12-31" }),
        ),
        (
            "time",
            json!({ "type": "time", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "default": "09:30", "min": "00:00",
            "max": "23:59" }),
        ),
        (
            "datetime",
            json!({ "type": "datetime", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "default": "2024-05-01T09:30",
            "min": "2024-01-01T00:00", "max": "2024-12-31T23:59" }),
        ),
        (
            "radio",
            json!({ "type": "radio", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "options": options, "default": "a" }),
        ),
        (
            "select",
            json!({ "type": "select", "name": "f", "label": "L", "description": "D",
            "required": true, "visible_when": when, "options": options, "default": "b",
            "placeholder": "p" }),
        ),
        (
            "checkboxes",
            json!({ "type": "checkboxes", "name": "f", "label": "L",
            "description": "D", "required": true, "visible_when": when, "options": options,
            "default": ["a"], "min_selected": 1, "max_selected": 2 }),
        ),
        (
            "toggle",
            json!({ "type": "toggle", "name": "f", "label": "L", "description": "D",
            "visible_when": when, "default": true }),
        ),
        (
            "slider",
            json!({ "type": "slider", "name": "f", "label": "L", "description": "D",
            "visible_when": when, "min": 0, "max": 10, "step": 2, "default": 4 }),
        ),
        (
            "submit",
            json!({ "type": "submit", "label": "Go", "name": "go", "value": "yes",
            "visible_when": when }),
        ),
        (
            "choices",
            json!({ "type": "choices", "name": "go", "label": "L", "options": options,
            "visible_when": when }),
        ),
        (
            "columns",
            json!({ "type": "columns", "visible_when": when, "children": [
                { "type": "markdown", "text": "x" }, { "type": "markdown", "text": "y" },
            ] }),
        ),
        (
            "tabs",
            json!({ "type": "tabs", "visible_when": when, "tabs": [
                { "label": "A", "children": [{ "type": "markdown", "text": "x" }] },
                { "label": "B", "children": [{ "type": "markdown", "text": "y" }] },
            ] }),
        ),
        (
            "divider",
            json!({ "type": "divider", "label": "More", "visible_when": when }),
        ),
        (
            "callout",
            json!({ "type": "callout", "text": "x", "title": "T", "tone": "warning",
            "visible_when": when }),
        ),
        (
            "key_values",
            json!({ "type": "key_values", "visible_when": when, "items": [
                { "key": "a", "value": "x" }, { "key": "b", "value": 2 },
                { "key": "c", "value": true },
            ] }),
        ),
        (
            "stat",
            json!({ "type": "stat", "label": "L", "value": 42, "change": "+3", "trend": "up",
            "sentiment": "positive", "visible_when": when }),
        ),
        (
            "code",
            json!({ "type": "code", "code": "fn main() {}", "language": "rust", "label": "main.rs",
            "visible_when": when }),
        ),
        (
            "diff",
            json!({ "type": "diff", "diff": "-a\n+b", "label": "f", "visible_when": when }),
        ),
        (
            "copyable",
            json!({ "type": "copyable", "text": "pnpm i", "label": "L", "monospace": false,
            "visible_when": when }),
        ),
        (
            "table",
            json!({ "type": "table", "caption": "C", "sortable": false, "filterable": true,
            "visible_when": when,
            "columns": [{ "label": "Name" }, { "label": "Count", "align": "end" }],
            "rows": [["a", 1], [null, 2.5], [true, null]] }),
        ),
        (
            "chart",
            json!({ "type": "chart", "summary": "S", "height": 240, "visible_when": when,
            "option": { "series": [{ "type": "pie", "data": [{ "name": "a", "value": 1 }] }] } }),
        ),
    ]
}

/// Every kind of the table is accepted with every property its schema lists, and the schema lists
/// exactly the properties the example has, so a property cannot be added to a type or a schema
/// alone, and the kinds of the table are the kinds of `Component`, each of which names itself
/// (`Component::kind`) as the table does and has its `visible_when` read by
/// `Component::visible_when`.
#[test]
fn every_kind_agrees_with_its_schema() {
    let examples = full_examples();
    let names: Vec<&str> = KINDS.iter().map(|kind| kind.name).collect();
    assert_eq!(
        examples.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        names
    );
    for (name, example) in examples {
        let page = json!({ "body": [
            { "type": "text", "name": "other", "label": "Other" },
            example.clone(),
            { "type": "submit" },
        ] });
        let content = parse(page.clone()).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(serde_json::to_value(&content).unwrap(), page, "{name}");
        assert_eq!(content.body[1].kind(), name);
        assert_eq!(
            content.body[1]
                .visible_when()
                .map(|when| when.field.as_str()),
            Some("other"),
            "{name}"
        );
        let schema = schema_named(name).unwrap();
        let mut listed: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
        let mut given: Vec<&String> = example.as_object().unwrap().keys().collect();
        listed.sort();
        given.sort();
        assert_eq!(listed, given, "{name}");
        assert_eq!(schema["properties"]["type"]["const"], name);
        assert_eq!(schema["additionalProperties"], false, "{name}");
    }
}

/// `schema_named` serves the page object too, and nothing else.
#[test]
fn schemas_are_served_by_kind_and_for_the_page() {
    let page = schema_named("page").unwrap();
    assert_eq!(page["required"], json!(["body"]));
    assert_eq!(
        page["properties"]["body"]["items"]["properties"]["type"]["enum"]
            .as_array()
            .unwrap()
            .len(),
        KINDS.len()
    );
    assert!(schema_named("video").is_none());
}

/// A submission is an object of the page's fields and submitters, each a string, a number, a
/// boolean or an array of strings; anything else, or any submission of a page with no submitter,
/// is refused, and the order sent is kept.
#[test]
fn a_submission_is_checked_against_the_page() {
    let content = parse(json!({ "body": [
        { "type": "text", "name": "who", "label": "Who" },
        { "type": "section", "children": [
            { "type": "checkboxes", "name": "topics", "label": "T", "options": [{ "value": "a" }] },
        ] },
        { "type": "choices", "name": "action", "options": [{ "value": "ok" }] },
        { "type": "submit", "name": "alt", "value": "x" },
    ] }))
    .unwrap();
    let sent = json!({ "topics": ["a"], "who": "Ada", "action": "ok", "n": 1 });
    let error = content.check_submission(&sent).expect_err("n is no field");
    assert!(error.contains("`n` is not a field of this page"), "{error}");

    let sent = json!({ "topics": ["a"], "who": "Ada", "action": "ok", "alt": "x" });
    let checked = content.check_submission(&sent).expect("a submission");
    assert_eq!(
        checked.keys().collect::<Vec<_>>(),
        ["topics", "who", "action", "alt"]
    );
    content
        .check_submission(&json!({}))
        .expect("a submission of nothing");
    let report = parse(json!({ "body": [{ "type": "markdown", "text": "x" }] })).unwrap();
    let error = report.check_submission(&json!({})).expect_err("no form");
    assert!(error.contains("no form"), "{error}");

    for (data, fragment) in [
        (json!([]), "must be an object"),
        (json!("x"), "must be an object"),
        (
            json!({ "who": { "first": "Ada" } }),
            "`who` must be a string",
        ),
        (json!({ "who": null }), "`who` must be a string"),
        (json!({ "topics": [1] }), "`topics` must be a string"),
        (json!({ "topics": [["a"]] }), "`topics` must be a string"),
    ] {
        let error = content
            .check_submission(&data)
            .expect_err(&data.to_string());
        assert!(error.contains(fragment), "{data}: {error}");
    }
    for ok in [
        json!({ "who": 3 }),
        json!({ "who": true }),
        json!({ "who": 2.5 }),
    ] {
        content.check_submission(&ok).expect("a scalar");
    }
}

/// Fields may sit in columns: they are fields of the page, so their names are unique across it, a
/// `visible_when` elsewhere may name them, and a page whose only fields are in columns still needs
/// a submitter.
#[test]
fn a_form_can_be_laid_out_in_columns() {
    let page = json!({ "body": [
        { "type": "columns", "children": [
            { "type": "text", "name": "first", "label": "First" },
            { "type": "section", "children": [
                { "type": "toggle", "name": "more", "label": "More?" },
            ] },
        ] },
        { "type": "markdown", "text": "x", "visible_when": { "field": "more", "equals": true } },
        { "type": "columns", "children": [
            { "type": "markdown", "text": "y" },
            { "type": "submit", "label": "Go" },
        ] },
    ] });
    let content = parse(page.clone()).expect("a form in columns");
    assert_eq!(serde_json::to_value(&content).unwrap(), page);
    content
        .check_submission(&json!({ "first": "Ada", "more": true }))
        .expect("a submission carries the fields inside columns");

    let error = parse(json!({ "body": [
        { "type": "text", "name": "a", "label": "A" },
        { "type": "columns", "children": [
            { "type": "markdown", "text": "x" },
            { "type": "toggle", "name": "a", "label": "A" },
        ] },
        { "type": "submit" },
    ] }))
    .expect_err("a name used twice");
    assert_eq!(error.path, "body[1].children[1].name");
    assert_eq!(error.kind, Some("toggle"));
    assert!(error.message.contains("already the name"), "{error}");

    let error = parse(json!({ "body": [
        { "type": "columns", "children": [
            { "type": "text", "name": "a", "label": "A" },
            { "type": "markdown", "text": "x" },
        ] },
    ] }))
    .expect_err("fields in columns without a submitter");
    assert_eq!(error.path, "body");
    assert_eq!(error.kind, None);
    assert!(
        error.message.contains("needs a `submit` or `choices`"),
        "{error}"
    );
}

/// A tab holds no input field and no submitter at any depth, and the refusal names the offending
/// component; the other kinds are welcome in a tab, nested layout included.
#[test]
fn a_tab_holds_no_input_field_and_no_submitter() {
    let md = json!({ "type": "markdown", "text": "x" });
    let tabs = |children: Vec<Value>| {
        json!({ "type": "tabs", "tabs": [
            { "label": "A", "children": [md.clone()] },
            { "label": "B", "children": children },
        ] })
    };
    let offenders = [
        (json!({ "type": "text", "name": "t", "label": "T" }), "text"),
        (
            json!({ "type": "toggle", "name": "t", "label": "T" }),
            "toggle",
        ),
        (json!({ "type": "submit" }), "submit"),
        (
            json!({ "type": "choices", "name": "c", "options": [{ "value": "a" }] }),
            "choices",
        ),
    ];
    for (offender, kind) in offenders {
        let at = |page: Value, path: &str| {
            let error = parse(page.clone()).expect_err(&format!("{kind}: {page}"));
            assert_eq!(error.path, path, "{kind}: {error}");
            assert_eq!(error.kind, Some("tabs"), "{kind}: {error}");
            assert_eq!(error.schema(), schema_named("tabs").unwrap(), "{kind}");
        };
        // Directly in the tab, inside a section or columns in the tab, and in tabs in the tab.
        at(
            json!({ "body": [tabs(vec![offender.clone()])] }),
            "body[0].tabs[1].children[0]",
        );
        at(
            json!({ "body": [tabs(vec![
                md.clone(),
                json!({ "type": "section", "children": [offender.clone()] }),
            ])] }),
            "body[0].tabs[1].children[1].children[0]",
        );
        at(
            json!({ "body": [tabs(vec![
                md.clone(),
                json!({ "type": "columns", "children": [md.clone(), offender.clone()] }),
            ])] }),
            "body[0].tabs[1].children[1].children[1]",
        );
        at(
            json!({ "body": [tabs(vec![
                md.clone(),
                json!({ "type": "tabs", "tabs": [
                    { "label": "C", "children": [md.clone()] },
                    { "label": "D", "children": [offender.clone()] },
                ] }),
            ])] }),
            "body[0].tabs[1].children[1].tabs[1].children[0]",
        );
    }

    // A tab full of display components, layout included, is fine.
    let page = json!({ "body": [
        { "type": "text", "name": "t", "label": "T" },
        tabs(vec![
            md.clone(),
            json!({ "type": "section", "children": [
                { "type": "callout", "text": "x" },
                { "type": "columns", "children": [md.clone(), md.clone()] },
            ] }),
        ]),
        { "type": "submit" },
    ] });
    parse(page).expect("display components in tabs");
}

/// A component in a tab can depend on a field outside the tabs.
#[test]
fn a_tab_can_depend_on_a_field_outside_it() {
    let page = json!({ "body": [
        { "type": "toggle", "name": "more", "label": "More?" },
        { "type": "tabs", "tabs": [
            { "label": "A", "children": [{ "type": "markdown", "text": "x" }] },
            { "label": "B", "children": [
                { "type": "callout", "text": "y",
                  "visible_when": { "field": "more", "equals": true } },
            ] },
        ] },
        { "type": "submit" },
    ] });
    let content = parse(page.clone()).expect("a dependent component in a tab");
    assert_eq!(serde_json::to_value(&content).unwrap(), page);
}

/// A container cannot depend on a field inside it, which would hide the field that shows it, but
/// what it holds can depend on a field beside it in the container.
#[test]
fn a_component_in_a_container_can_depend_on_a_field_beside_it() {
    let page = json!({ "body": [
        { "type": "section", "children": [
            { "type": "toggle", "name": "more", "label": "More?" },
            { "type": "text", "name": "why", "label": "Why?",
              "visible_when": { "field": "more", "equals": true } },
        ] },
        { "type": "submit" },
    ] });
    parse(page).expect("a dependent field beside its condition");
}

/// A text field's `pattern` must compile as the JavaScript regular expression the panel tests the
/// whole value with, under any of the flags it tries, and is at most 512 characters.
#[test]
fn a_text_pattern_must_compile_and_stay_short() {
    let page = |pattern: &str| {
        parse(form(vec![
            json!({ "type": "text", "name": "t", "label": "T", "pattern": pattern }),
        ]))
    };
    for good in [
        "[a-z]+",
        r"\d{3}-\d{4}",
        r"\p{L}+",
        r"[\p{L}--[a-z]]+",
        "a|b",
    ] {
        page(good).unwrap_or_else(|error| panic!("{good}: {error}"));
    }
    for bad in ["(", "[a-", "a{2,1}", r"(?<n>a)(?<n>b)", "*"] {
        let error = page(bad).expect_err(bad);
        assert_eq!(error.path, "body[0].pattern", "{bad}");
        assert_eq!(error.schema_name(), "text", "{bad}");
        assert!(
            error
                .message
                .starts_with("is not a JavaScript regular expression"),
            "{bad}: {}",
            error.message
        );
    }
    page(&"a".repeat(512)).expect("512 characters");
    let error = page(&"a".repeat(513)).expect_err("513 characters");
    assert_eq!(
        (error.path.as_str(), error.message.as_str()),
        ("body[0].pattern", "must have at most 512 characters")
    );
}

/// A text field's `default` must match its own `pattern`, tested the way the panel tests a value.
#[test]
fn a_text_default_must_match_its_own_pattern() {
    let page = |default: &str| {
        parse(form(vec![
            json!({ "type": "text", "name": "t", "label": "T", "pattern": "[a-z]+",
                "default": default }),
        ]))
    };
    page("abc").expect("a default that matches");
    for bad in ["ABC", "abc1", "a b"] {
        let error = page(bad).expect_err(bad);
        assert_eq!(
            (
                error.path.as_str(),
                error.message.as_str(),
                error.schema_name()
            ),
            ("body[0].default", "does not match its own pattern", "text"),
            "{bad}"
        );
    }
}

/// A blank `default` is an empty answer, which the panel never tests against the pattern, so it is
/// accepted whatever the pattern is; a non-blank one that fails the pattern is still refused.
#[test]
fn a_blank_text_default_passes_any_pattern() {
    let page = |default: &str| {
        parse(form(vec![
            json!({ "type": "text", "name": "t", "label": "T", "pattern": "[a-z]+",
                "default": default }),
        ]))
    };
    for blank in ["", " ", "   ", "\t\n"] {
        page(blank).unwrap_or_else(|error| panic!("{blank:?}: {error}"));
    }
    let error = page(" 1 ").expect_err("a default with content that fails the pattern");
    assert_eq!(error.path, "body[0].default");
}

/// A count beyond what the page's types hold is refused by the schema at its path, not by the
/// reading of the page into its types after the schema accepted it.
#[test]
fn a_count_is_bounded_by_the_schema() {
    let too_many = u64::from(u32::MAX) + 1;
    for (component, at) in [
        (
            json!({ "type": "text", "name": "f", "label": "L", "max_length": too_many }),
            "max_length",
        ),
        (
            json!({ "type": "textarea", "name": "f", "label": "L", "min_length": too_many }),
            "min_length",
        ),
        (
            json!({ "type": "checkboxes", "name": "f", "label": "L", "options": [{ "value": "a" }],
                    "max_selected": too_many }),
            "max_selected",
        ),
    ] {
        let error = parse(form(vec![component])).expect_err(at);
        assert_eq!(error.path, format!("body[0].{at}"));
        assert!(
            error.message.starts_with("must be at most"),
            "{}",
            error.message
        );
    }
}

/// Columns are two to four: a cross-check on top of the schema's lower bound.
#[test]
fn columns_hold_two_to_four_children() {
    let md = json!({ "type": "markdown", "text": "x" });
    for count in 2..=4 {
        let children = vec![md.clone(); count];
        parse(json!({ "body": [{ "type": "columns", "children": children }] }))
            .unwrap_or_else(|error| panic!("{count} columns: {error}"));
    }
    for count in [1, 5, 6] {
        let children = vec![md.clone(); count];
        let error = parse(json!({ "body": [{ "type": "columns", "children": children }] }))
            .expect_err(&format!("{count} columns"));
        assert_eq!(error.path, "body[0].children", "{count}: {error}");
        assert_eq!(error.kind, Some("columns"), "{count}: {error}");
    }
}

/// Every row has as many cells as the table has columns, and the refusal says which row and the
/// two counts; a table with no rows and every cell type are accepted.
#[test]
fn a_table_row_has_a_cell_for_every_column() {
    let table = |rows: Value| {
        json!({ "body": [{ "type": "table",
            "columns": [{ "label": "A" }, { "label": "B", "align": "end" }],
            "rows": rows }] })
    };
    parse(table(json!([]))).expect("a table with no rows");
    let page = table(json!([["a", 1], [2.5, true], [null, null]]));
    let content = parse(page.clone()).expect("every cell type");
    assert_eq!(serde_json::to_value(&content).unwrap(), page);

    for (rows, row, cells) in [
        (json!([["a", 1], ["b"]]), 1, 1),
        (json!([["a", 1, 2]]), 0, 3),
        (json!([[]]), 0, 0),
    ] {
        let error = parse(table(rows.clone())).expect_err(&rows.to_string());
        assert_eq!(
            error.path,
            format!("body[0].rows[{row}]"),
            "{rows}: {error}"
        );
        assert_eq!(error.kind, Some("table"), "{rows}: {error}");
        assert!(
            error.message.contains(&cells.to_string()),
            "{rows}: {error}"
        );
        assert!(error.message.contains('2'), "{rows}: {error}");
        assert_eq!(error.schema(), schema_named("table").unwrap());
    }
}
