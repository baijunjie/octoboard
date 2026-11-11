//! The kinds of component a page has, in one table: each with the JSON Schema `show_page` enforces
//! on it and the guide tool hands an agent. A new kind is a row of [`KINDS`] (with its type in
//! `super` and its arm in `super::rules`).

use serde_json::{json, Map, Value};

/// One kind of component: the `type` that names it and the schema of an object of that kind.
pub(super) struct Kind {
    pub name: &'static str,
    pub schema: fn() -> Value,
}

/// Every kind a page may use, in the order the guide lists them.
pub(super) const KINDS: &[Kind] = &[
    Kind {
        name: "section",
        schema: section,
    },
    Kind {
        name: "markdown",
        schema: markdown,
    },
    Kind {
        name: "text",
        schema: text,
    },
    Kind {
        name: "textarea",
        schema: textarea,
    },
    Kind {
        name: "number",
        schema: number,
    },
    Kind {
        name: "date",
        schema: date,
    },
    Kind {
        name: "time",
        schema: time,
    },
    Kind {
        name: "datetime",
        schema: datetime,
    },
    Kind {
        name: "radio",
        schema: radio,
    },
    Kind {
        name: "select",
        schema: select,
    },
    Kind {
        name: "checkboxes",
        schema: checkboxes,
    },
    Kind {
        name: "toggle",
        schema: toggle,
    },
    Kind {
        name: "slider",
        schema: slider,
    },
    Kind {
        name: "submit",
        schema: submit,
    },
    Kind {
        name: "choices",
        schema: choices,
    },
    Kind {
        name: "columns",
        schema: columns,
    },
    Kind {
        name: "tabs",
        schema: tabs,
    },
    Kind {
        name: "divider",
        schema: divider,
    },
    Kind {
        name: "callout",
        schema: callout,
    },
    Kind {
        name: "key_values",
        schema: key_values,
    },
    Kind {
        name: "stat",
        schema: stat,
    },
    Kind {
        name: "code",
        schema: code,
    },
    Kind {
        name: "diff",
        schema: diff,
    },
    Kind {
        name: "copyable",
        schema: copyable,
    },
    Kind {
        name: "table",
        schema: table,
    },
    Kind {
        name: "chart",
        schema: super::chart::chart,
    },
];

/// The name under which [`schema_named`] gives the schema of the page object itself.
pub const PAGE_SCHEMA_NAME: &str = "page";

/// The `type` of every kind, in the guide's order.
pub fn kind_names() -> impl Iterator<Item = &'static str> {
    KINDS.iter().map(|kind| kind.name)
}

/// The schema of the kind `name`, or of the page object for [`PAGE_SCHEMA_NAME`].
pub fn schema_named(name: &str) -> Option<Value> {
    if name == PAGE_SCHEMA_NAME {
        return Some(page_schema());
    }
    KINDS
        .iter()
        .find(|kind| kind.name == name)
        .map(|kind| (kind.schema)())
}

/// What a property that holds components says of them. The `x-component` mark is what the
/// validator (`super::check`) follows into the kind's own schema.
fn components(description: &str, min_items: u32) -> Value {
    json!({
        "type": "array",
        "minItems": min_items,
        "description": description,
        "items": {
            "type": "object",
            "x-component": true,
            "description": "A component: an object whose `type` names its kind.",
            "properties": { "type": { "enum": kind_names().collect::<Vec<_>>() } },
            "required": ["type"],
        },
    })
}

pub(super) fn page_schema() -> Value {
    json!({
        "type": "object",
        "description": "A page: an optional title and the components of its body, top to bottom.",
        "properties": {
            "title": {
                "type": "string",
                "minLength": 1,
                "description": "The page's title, as `list_pages` shows it.",
            },
            "body": components("The page's components, in order.", 1),
        },
        "required": ["body"],
        "additionalProperties": false,
    })
}

pub(super) fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

pub(super) fn non_blank(description: &str) -> Value {
    json!({ "type": "string", "minLength": 1, "description": description })
}

/// A count the page's types hold as a `u32`: the schema bounds it to that, so that a page the
/// schema accepts is never one the types cannot read.
fn count(description: &str, minimum: u32) -> Value {
    json!({
        "type": "integer",
        "minimum": minimum,
        "maximum": u32::MAX,
        "description": description,
    })
}

pub(super) fn number_property(description: &str) -> Value {
    json!({ "type": "number", "description": description })
}

fn step() -> Value {
    json!({ "type": "number", "exclusiveMinimum": 0, "description": "The step between values." })
}

fn visible_when() -> Value {
    json!({
        "type": "object",
        "description": "Shows this component only while the input field named `field` holds \
                        `equals` (a multiple choice: while its selection contains it). A hidden \
                        component's fields are not submitted.",
        "properties": {
            "field": non_blank("The `name` of an input field on this page outside this component: not its own, nor one inside it."),
            "equals": {
                "type": ["string", "number", "boolean"],
                "description": "The value that shows the component.",
            },
        },
        "required": ["field", "equals"],
        "additionalProperties": false,
    })
}

fn options() -> Value {
    json!({
        "type": "array",
        "minItems": 1,
        "description": "The options, with distinct values.",
        "items": {
            "type": "object",
            "properties": {
                "value": non_blank("What is submitted when this option is chosen."),
                "label": string("What the user reads; the value when left out."),
            },
            "required": ["value"],
            "additionalProperties": false,
        },
    })
}

/// The schema of one kind: the properties every kind has (`type`, `visible_when`) and its own.
pub(super) fn kind_schema(name: &str, description: &str, own: Value, required: &[&str]) -> Value {
    let mut properties = Map::new();
    properties.insert("type".to_string(), json!({ "const": name }));
    if let Value::Object(own) = own {
        properties.extend(own);
    }
    properties.insert("visible_when".to_string(), visible_when());
    let mut all_required = vec!["type"];
    all_required.extend_from_slice(required);
    json!({
        "type": "object",
        "description": description,
        "properties": properties,
        "required": all_required,
        "additionalProperties": false,
    })
}

/// [`kind_schema`] for an input field, which names the field and labels it.
fn input_schema(
    name: &str,
    description: &str,
    own: Value,
    required: &[&str],
    has_required: bool,
) -> Value {
    let mut properties = Map::new();
    properties.insert(
        "name".to_string(),
        non_blank("The key this field is submitted under; unique among the page's input fields."),
    );
    properties.insert(
        "label".to_string(),
        non_blank("What the user reads beside the field."),
    );
    properties.insert(
        "description".to_string(),
        string("Help text under the field."),
    );
    if has_required {
        properties.insert(
            "required".to_string(),
            json!({ "type": "boolean", "description": "Whether a value must be given." }),
        );
    }
    if let Value::Object(own) = own {
        properties.extend(own);
    }
    let mut all_required = vec!["name", "label"];
    all_required.extend_from_slice(required);
    kind_schema(name, description, Value::Object(properties), &all_required)
}

fn section() -> Value {
    kind_schema(
        "section",
        "Groups components under an optional heading.",
        json!({
            "heading": string("The section's heading."),
            "children": components("The components inside the section.", 1),
        }),
        &["children"],
    )
}

fn markdown() -> Value {
    kind_schema(
        "markdown",
        "GitHub-flavoured Markdown text. Raw HTML is shown as plain text, an image shows only \
         from a `data:` URL, and an external link opens in the system browser.",
        json!({ "text": non_blank("The Markdown source.") }),
        &["text"],
    )
}

fn text() -> Value {
    input_schema(
        "text",
        "A single-line text field. Submitted as a string.",
        json!({
            "default": string("The value it starts with."),
            "placeholder": string("Hint shown while it is empty."),
            "pattern": {
                "type": "string",
                "maxLength": MOST_PATTERN_CHARACTERS,
                "description": "A JavaScript regular expression the whole value must match; one that does not compile is refused.",
            },
            "min_length": count("The fewest characters.", 0),
            "max_length": count("The most characters.", 1),
        }),
        &[],
        true,
    )
}

fn textarea() -> Value {
    input_schema(
        "textarea",
        "A multi-line text field. Submitted as a string.",
        json!({
            "default": string("The value it starts with."),
            "placeholder": string("Hint shown while it is empty."),
            "min_length": count("The fewest characters.", 0),
            "max_length": count("The most characters.", 1),
            "rows": json!({
                "type": "integer", "minimum": 1, "maximum": 40, "description": "Visible rows.",
            }),
        }),
        &[],
        true,
    )
}

fn number() -> Value {
    input_schema(
        "number",
        "A number field. Submitted as a number.",
        json!({
            "default": number_property("The value it starts with."),
            "min": number_property("The least value."),
            "max": number_property("The greatest value."),
            "step": step(),
        }),
        &[],
        true,
    )
}

/// The schema of the date, time and date-and-time kinds, which differ only in the format.
fn temporal(name: &str, description: &str, format: &str) -> Value {
    input_schema(
        name,
        description,
        json!({
            "default": string(&format!("The value it starts with, as {format}.")),
            "min": string(&format!("The earliest value, as {format}.")),
            "max": string(&format!("The latest value, as {format}.")),
        }),
        &[],
        true,
    )
}

fn date() -> Value {
    temporal("date", "A date field. Submitted as a string.", "YYYY-MM-DD")
}

fn time() -> Value {
    temporal(
        "time",
        "A time-of-day field. Submitted as a string.",
        "HH:MM",
    )
}

fn datetime() -> Value {
    temporal(
        "datetime",
        "A date-and-time field, local, with no zone. Submitted as a string.",
        "YYYY-MM-DDTHH:MM",
    )
}

fn radio() -> Value {
    input_schema(
        "radio",
        "A single choice shown as radio buttons. Submitted as the chosen option's value.",
        json!({
            "options": options(),
            "default": string("The value of the option chosen at the start."),
        }),
        &["options"],
        true,
    )
}

fn select() -> Value {
    input_schema(
        "select",
        "A single choice shown as a drop-down. Submitted as the chosen option's value.",
        json!({
            "options": options(),
            "default": string("The value of the option chosen at the start."),
            "placeholder": string("Shown while nothing is chosen."),
        }),
        &["options"],
        true,
    )
}

fn checkboxes() -> Value {
    input_schema(
        "checkboxes",
        "A multiple choice. Submitted as an array of the chosen values, in the options' order; \
         `required` means at least one.",
        json!({
            "options": options(),
            "default": {
                "type": "array",
                "items": { "type": "string" },
                "description": "The values of the options chosen at the start.",
            },
            "min_selected": count("The fewest options to choose.", 0),
            "max_selected": count("The most options to choose.", 1),
        }),
        &["options"],
        true,
    )
}

fn toggle() -> Value {
    input_schema(
        "toggle",
        "An on/off switch. Submitted as a boolean.",
        json!({ "default": json!({ "type": "boolean", "description": "Whether it starts on." }) }),
        &[],
        false,
    )
}

fn slider() -> Value {
    input_schema(
        "slider",
        "A range slider. Submitted as a number.",
        json!({
            "min": number_property("The least value."),
            "max": number_property("The greatest value, above `min`."),
            "step": step(),
            "default": number_property("The value it starts at, within the range."),
        }),
        &["min", "max"],
        false,
    )
}

fn submit() -> Value {
    kind_schema(
        "submit",
        "A button that submits the form. With `name` and `value` it adds `name: value` to the \
         submission.",
        json!({
            "label": string("The button's text; \"Submit\" when left out."),
            "name": non_blank("The key it adds to the submission; with `value`, and not an input field's name."),
            "value": string("The value it adds; with `name`."),
        }),
        &[],
    )
}

fn choices() -> Value {
    kind_schema(
        "choices",
        "A group of buttons, each of which submits the form with one press and adds `name: <the \
         pressed option's value>` to the submission.",
        json!({
            "name": non_blank("The key the pressed value is submitted under; unique among the submitters, and not an input field's name."),
            "label": string("A caption over the buttons."),
            "options": options(),
        }),
        &["name", "options"],
    )
}

/// The longest `pattern` a text field takes, in characters.
const MOST_PATTERN_CHARACTERS: u32 = 512;

/// The most columns a `columns` lays side by side; `super::rules` holds a page to it.
pub(super) const MOST_COLUMNS: usize = 4;

fn plain_value(description: &str, types: &[&str]) -> Value {
    json!({ "type": types, "description": description })
}

pub(super) fn flag(description: &str) -> Value {
    json!({ "type": "boolean", "description": description })
}

fn columns() -> Value {
    kind_schema(
        "columns",
        "Lays its children side by side, each child one column (a `section` for a column of \
         several components); the columns stack one above the other where the panel is narrow.",
        json!({
            "children": components(
                &format!("The columns, left to right in the reading direction; 2 to {MOST_COLUMNS}."),
                2,
            ),
        }),
        &["children"],
    )
}

fn tabs() -> Value {
    kind_schema(
        "tabs",
        "Tabs, one of which shows at a time, the first at the start. A tab holds no input field \
         and no submitter: a form stays outside tabs, where all of it can be seen.",
        json!({
            "tabs": {
                "type": "array",
                "minItems": 2,
                "description": "The tabs, in order.",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": non_blank("The tab's name."),
                        "children": components("What the tab shows.", 1),
                    },
                    "required": ["label", "children"],
                    "additionalProperties": false,
                },
            },
        }),
        &["tabs"],
    )
}

fn divider() -> Value {
    kind_schema(
        "divider",
        "A thin horizontal rule between blocks, with an optional label on it.",
        json!({
            "label": non_blank("A short text on the rule, such as the name of the part that follows."),
        }),
        &[],
    )
}

fn callout() -> Value {
    kind_schema(
        "callout",
        "A highlighted note: information, a success, a warning or a danger.",
        json!({
            "text": non_blank("The note, as Markdown (the same as a `markdown` block's)."),
            "title": non_blank("A bold line above the note."),
            "tone": {
                "enum": ["info", "success", "warning", "danger"],
                "description": "Its colour and icon; `info` when left out.",
            },
        }),
        &["text"],
    )
}

fn key_values() -> Value {
    kind_schema(
        "key_values",
        "A list of keys, each with its value, such as the facts of a release or a run.",
        json!({
            "items": {
                "type": "array",
                "minItems": 1,
                "description": "The pairs, in order.",
                "items": {
                    "type": "object",
                    "properties": {
                        "key": non_blank("What the value is."),
                        "value": plain_value("The value.", &["string", "number", "boolean"]),
                    },
                    "required": ["key", "value"],
                    "additionalProperties": false,
                },
            },
        }),
        &["items"],
    )
}

fn stat() -> Value {
    kind_schema(
        "stat",
        "A figure with its label and an optional change, such as a count or a duration; put \
         several in a `columns` to show them in a row.",
        json!({
            "label": non_blank("What the figure counts or measures."),
            "value": plain_value("The figure, as shown: a number, or a string with its unit.", &["string", "number"]),
            "change": non_blank("The change, as shown, such as \"+12%\" or \"-3 since Monday\"."),
            "trend": {
                "enum": ["up", "down", "flat"],
                "description": "Which way it changed, drawn as an arrow beside `change`.",
            },
            "sentiment": {
                "enum": ["positive", "negative", "neutral"],
                "description": "Whether the change is good news, colouring it; `neutral` when left out.",
            },
        }),
        &["label", "value"],
    )
}

fn code() -> Value {
    kind_schema(
        "code",
        "A block of code, highlighted, with a button that copies it.",
        json!({
            "code": non_blank("The code, as it is to be copied."),
            "language": non_blank("The language to highlight it as, such as `rust` or `bash`."),
            "label": non_blank("A caption above it, such as a file name."),
        }),
        &["code"],
    )
}

fn diff() -> Value {
    kind_schema(
        "diff",
        "A unified diff (as `git diff` prints it), its added and removed lines marked, with a \
         button that copies it.",
        json!({
            "diff": non_blank("The unified diff."),
            "label": non_blank("A caption above it, such as a file name."),
        }),
        &["diff"],
    )
}

fn copyable() -> Value {
    kind_schema(
        "copyable",
        "A block of text with a button that copies exactly that text and confirms it, such as \
         a command to run, an address or a token.",
        json!({
            "text": non_blank("The text shown and copied."),
            "label": non_blank("A caption above it."),
            "monospace": flag("Whether it is shown in a monospaced font; true when left out."),
        }),
        &["text"],
    )
}

fn table() -> Value {
    kind_schema(
        "table",
        "A table of plain values. The user can sort it by a column and filter its rows by text.",
        json!({
            "caption": non_blank("What the table shows, above it."),
            "columns": {
                "type": "array",
                "minItems": 1,
                "description": "The columns, in order.",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": non_blank("The column's header."),
                        "align": {
                            "enum": ["start", "end"],
                            "description": "Which side its values sit on; `end` when left out for a column of numbers, else `start`.",
                        },
                    },
                    "required": ["label"],
                    "additionalProperties": false,
                },
            },
            "rows": {
                "type": "array",
                "description": "The rows, each an array of one value per column, in the columns' order; `null` for an empty cell.",
                "items": {
                    "type": "array",
                    "items": plain_value("A cell.", &["string", "number", "boolean", "null"]),
                },
            },
            "sortable": flag("Whether the user can sort by a column; true when left out."),
            "filterable": flag("Whether a filter field is shown above it; true when left out."),
        }),
        &["columns", "rows"],
    )
}
