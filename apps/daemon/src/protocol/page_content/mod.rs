//! The content of a report page: a tree of components a session pushes with `show_page`, which the
//! store keeps and the panel renders, and the submission a form on it comes back with. See
//! "Page content" in `apps/daemon/PROTOCOL.md`.
//!
//! The types here are the wire form. [`kinds`] holds, in one table, each kind of component with
//! its JSON Schema, which is what `show_page` enforces and what the guide tool hands an agent;
//! [`check`] is the validator that reads that table, [`rules`] the cross-checks a schema cannot
//! state, and [`submission`] what a form's submission may carry. A new kind is a new variant of
//! [`Component`], a new struct, a row of `kinds::KINDS`, its arms in the accessors of
//! [`Component`] and, for an input field or a submitter, in `rules`.

use serde::{Deserialize, Serialize};
use serde_json::{Number, Value};

mod chart;
mod check;
mod kinds;
mod rules;
mod submission;
#[cfg(test)]
mod tests;

pub use chart::Chart;
pub use check::PageContentError;
#[cfg(test)]
pub use kinds::kind_names;
pub use kinds::schema_named;

impl PageContent {
    /// Reads `value`, the page as an agent sent it, into a page: refused, with the location and the
    /// kind whose schema it breaks, when it does not match the schemas of its kinds or breaks a
    /// rule they cannot state.
    pub fn parse(value: &Value) -> Result<Self, PageContentError> {
        check::check_page(value)?;
        let content: Self = serde_json::from_value(value.clone())
            .map_err(|error| PageContentError::new("", error.to_string(), None))?;
        rules::cross_check(&content)?;
        Ok(content)
    }

    /// What a form on this page may be submitted with: `data` must be an object whose keys are
    /// names of the page's input fields and submitters and whose values are strings, numbers,
    /// booleans or arrays of strings.
    pub fn check_submission(&self, data: &Value) -> Result<Submission, String> {
        submission::check_submission(self, data)
    }
}

/// What a form on a page was submitted with: one key per submitted field, in the order the form
/// sent them, each a string, a number, a boolean or an array of strings (a [`SubmittedValue`]).
pub type Submission = serde_json::Map<String, serde_json::Value>;

/// One value of a [`Submission`]: the text of a text-like field or the pressed button's value, a
/// number, a toggle, or the chosen values of a multiple choice in the order of its options.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SubmittedValue {
    Text(String),
    Flag(bool),
    Number(PageNumber),
    Choices(Vec<String>),
}

/// A number as the page carries it: a JSON number, an integer staying an integer. A plain `number`
/// in the TypeScript bindings and the schema (see `protocol::bindings` and `protocol::schema`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PageNumber(pub Number);

/// A page: an optional title and the components that make up its body, top to bottom.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub title: Option<String>,
    pub body: Vec<Component>,
}

/// One component of a page: an object of the kind its `type` names, with that kind's properties
/// beside it. Every kind takes an optional `visible_when`.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Component {
    Section {
        #[serde(flatten)]
        component: Section,
    },
    Markdown {
        #[serde(flatten)]
        component: Markdown,
    },
    Text {
        #[serde(flatten)]
        component: TextField,
    },
    Textarea {
        #[serde(flatten)]
        component: TextareaField,
    },
    Number {
        #[serde(flatten)]
        component: NumberField,
    },
    Date {
        #[serde(flatten)]
        component: TemporalField,
    },
    Time {
        #[serde(flatten)]
        component: TemporalField,
    },
    Datetime {
        #[serde(flatten)]
        component: TemporalField,
    },
    Radio {
        #[serde(flatten)]
        component: ChoiceField,
    },
    Select {
        #[serde(flatten)]
        component: SelectField,
    },
    Checkboxes {
        #[serde(flatten)]
        component: CheckboxesField,
    },
    Toggle {
        #[serde(flatten)]
        component: ToggleField,
    },
    Slider {
        #[serde(flatten)]
        component: SliderField,
    },
    Submit {
        #[serde(flatten)]
        component: SubmitButton,
    },
    Choices {
        #[serde(flatten)]
        component: ChoiceButtons,
    },
    Columns {
        #[serde(flatten)]
        component: ColumnsLayout,
    },
    Tabs {
        #[serde(flatten)]
        component: TabsLayout,
    },
    Divider {
        #[serde(flatten)]
        component: DividerRule,
    },
    Callout {
        #[serde(flatten)]
        component: CalloutBlock,
    },
    KeyValues {
        #[serde(flatten)]
        component: KeyValueList,
    },
    Stat {
        #[serde(flatten)]
        component: StatFigure,
    },
    Code {
        #[serde(flatten)]
        component: CodeBlock,
    },
    Diff {
        #[serde(flatten)]
        component: DiffBlock,
    },
    Copyable {
        #[serde(flatten)]
        component: CopyableBlock,
    },
    Table {
        #[serde(flatten)]
        component: DataTable,
    },
    Chart {
        #[serde(flatten)]
        component: Chart,
    },
}

impl Component {
    /// The `type` that names this component's kind, as `kinds::KINDS` lists it.
    pub fn kind(&self) -> &'static str {
        match self {
            Component::Section { .. } => "section",
            Component::Markdown { .. } => "markdown",
            Component::Text { .. } => "text",
            Component::Textarea { .. } => "textarea",
            Component::Number { .. } => "number",
            Component::Date { .. } => "date",
            Component::Time { .. } => "time",
            Component::Datetime { .. } => "datetime",
            Component::Radio { .. } => "radio",
            Component::Select { .. } => "select",
            Component::Checkboxes { .. } => "checkboxes",
            Component::Toggle { .. } => "toggle",
            Component::Slider { .. } => "slider",
            Component::Submit { .. } => "submit",
            Component::Choices { .. } => "choices",
            Component::Columns { .. } => "columns",
            Component::Tabs { .. } => "tabs",
            Component::Divider { .. } => "divider",
            Component::Callout { .. } => "callout",
            Component::KeyValues { .. } => "key_values",
            Component::Stat { .. } => "stat",
            Component::Code { .. } => "code",
            Component::Diff { .. } => "diff",
            Component::Copyable { .. } => "copyable",
            Component::Table { .. } => "table",
            Component::Chart { .. } => "chart",
        }
    }

    /// The condition this component is shown on, if it has one.
    pub fn visible_when(&self) -> Option<&VisibleWhen> {
        match self {
            Component::Section { component } => component.visible_when.as_ref(),
            Component::Markdown { component } => component.visible_when.as_ref(),
            Component::Text { component } => component.visible_when.as_ref(),
            Component::Textarea { component } => component.visible_when.as_ref(),
            Component::Number { component } => component.visible_when.as_ref(),
            Component::Date { component }
            | Component::Time { component }
            | Component::Datetime { component } => component.visible_when.as_ref(),
            Component::Radio { component } => component.visible_when.as_ref(),
            Component::Select { component } => component.visible_when.as_ref(),
            Component::Checkboxes { component } => component.visible_when.as_ref(),
            Component::Toggle { component } => component.visible_when.as_ref(),
            Component::Slider { component } => component.visible_when.as_ref(),
            Component::Submit { component } => component.visible_when.as_ref(),
            Component::Choices { component } => component.visible_when.as_ref(),
            Component::Columns { component } => component.visible_when.as_ref(),
            Component::Tabs { component } => component.visible_when.as_ref(),
            Component::Divider { component } => component.visible_when.as_ref(),
            Component::Callout { component } => component.visible_when.as_ref(),
            Component::KeyValues { component } => component.visible_when.as_ref(),
            Component::Stat { component } => component.visible_when.as_ref(),
            Component::Code { component } => component.visible_when.as_ref(),
            Component::Diff { component } => component.visible_when.as_ref(),
            Component::Copyable { component } => component.visible_when.as_ref(),
            Component::Table { component } => component.visible_when.as_ref(),
            Component::Chart { component } => component.visible_when.as_ref(),
        }
    }

    /// The lists of components this one holds, each with its path from this component: the
    /// `children` of a section or of columns, the `children` of each tab. Empty for a component
    /// that holds none.
    pub fn children(&self) -> Vec<(String, &[Component])> {
        match self {
            Component::Section { component } => {
                vec![("children".to_string(), component.children.as_slice())]
            }
            Component::Columns { component } => {
                vec![("children".to_string(), component.children.as_slice())]
            }
            Component::Tabs { component } => component
                .tabs
                .iter()
                .enumerate()
                .map(|(at, tab)| (format!("tabs[{at}].children"), tab.children.as_slice()))
                .collect(),
            Component::Markdown { .. }
            | Component::Text { .. }
            | Component::Textarea { .. }
            | Component::Number { .. }
            | Component::Date { .. }
            | Component::Time { .. }
            | Component::Datetime { .. }
            | Component::Radio { .. }
            | Component::Select { .. }
            | Component::Checkboxes { .. }
            | Component::Toggle { .. }
            | Component::Slider { .. }
            | Component::Submit { .. }
            | Component::Choices { .. }
            | Component::Divider { .. }
            | Component::Callout { .. }
            | Component::KeyValues { .. }
            | Component::Stat { .. }
            | Component::Code { .. }
            | Component::Diff { .. }
            | Component::Copyable { .. }
            | Component::Table { .. }
            | Component::Chart { .. } => Vec::new(),
        }
    }
}

/// Shows its component only while the input field named `field` holds `equals`: a multiple
/// choice while its selection contains it, a toggle while it is that boolean.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VisibleWhen {
    pub field: String,
    pub equals: VisibleEquals,
}

/// What a [`VisibleWhen`] compares a field's value with.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VisibleEquals {
    Text(String),
    Flag(bool),
    Number(PageNumber),
}

/// One option of a choice: the `value` that is submitted, shown as `label` where there is one.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub value: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub label: Option<String>,
}

/// Declares a component's properties: the required ones, then, after the `;`, the optional ones,
/// which are left out of the serialization when absent.
macro_rules! component_struct {
    ($(#[$meta:meta])* $name:ident { $($required:ident: $required_type:ty),* ; $($optional:ident: $optional_type:ty),* $(,)? }) => {
        $(#[$meta])*
        #[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            $(pub $required: $required_type,)*
            $(
                #[serde(default, skip_serializing_if = "Option::is_none")]
                #[cfg_attr(test, ts(optional))]
                pub $optional: Option<$optional_type>,
            )*
        }
    };
}

/// [`component_struct`] for an input field, which has the properties every input has.
macro_rules! input_struct {
    ($(#[$meta:meta])* $name:ident {
        $($required:ident: $required_type:ty),* ; $($optional:ident: $optional_type:ty),* $(,)?
    }) => {
        component_struct!($(#[$meta])* $name {
            name: String, label: String $(, $required: $required_type)* ;
            description: String, required: bool, visible_when: VisibleWhen
            $(, $optional: $optional_type)*
        });
    };
}

component_struct! {
    /// Groups components under an optional heading.
    Section { children: Vec<Component>; heading: String, visible_when: VisibleWhen }
}

component_struct! {
    /// GitHub-flavoured Markdown. Raw HTML is shown as plain text, an image shows only from a `data:`
    /// URL and an external link opens in the system browser.
    Markdown { text: String; visible_when: VisibleWhen }
}

input_struct! {
    /// A single-line text field; `pattern` is the source of a JavaScript regular expression the
    /// whole value must match.
    TextField {
        ;
        default: String, placeholder: String, pattern: String, min_length: u32, max_length: u32,
    }
}

input_struct! {
    /// A multi-line text field.
    TextareaField {
        ;
        default: String, placeholder: String, min_length: u32, max_length: u32, rows: u32,
    }
}

input_struct! {
    /// A number field.
    NumberField { ; default: PageNumber, min: PageNumber, max: PageNumber, step: PageNumber }
}

input_struct! {
    /// A date field; `default`, `min` and `max` are `YYYY-MM-DD`. Also a time field (`HH:MM`) and a
    /// date-and-time field (`YYYY-MM-DDTHH:MM`, local, with no zone).
    TemporalField { ; default: String, min: String, max: String }
}

input_struct! {
    /// A single choice shown as radio buttons, one option selected.
    ChoiceField { options: Vec<ChoiceOption>; default: String }
}

input_struct! {
    /// A single choice shown as a drop-down.
    SelectField { options: Vec<ChoiceOption>; default: String, placeholder: String }
}

input_struct! {
    /// A multiple choice; the submitted array keeps the options' order. `required` means at least
    /// one is selected.
    CheckboxesField {
        options: Vec<ChoiceOption>; default: Vec<String>, min_selected: u32, max_selected: u32,
    }
}

component_struct! {
    /// An on/off switch, submitted as a boolean.
    ToggleField {
        name: String, label: String;
        description: String, visible_when: VisibleWhen, default: bool
    }
}

component_struct! {
    /// A range slider, submitted as a number.
    SliderField {
        name: String, label: String, min: PageNumber, max: PageNumber;
        description: String, visible_when: VisibleWhen, step: PageNumber, default: PageNumber
    }
}

component_struct! {
    /// Submits the form. Named, it adds `name: value` to the submission.
    SubmitButton { ; label: String, name: String, value: String, visible_when: VisibleWhen }
}

component_struct! {
    /// A group of buttons, each of which submits the form with one press and adds `name: <the
    /// pressed option's value>` to the submission.
    ChoiceButtons {
        name: String, options: Vec<ChoiceOption>;
        label: String, visible_when: VisibleWhen
    }
}

/// A plain value a report shows: a string, a number or a boolean, as a table cell or a key–value
/// pair's value holds it.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CellValue {
    Text(String),
    Flag(bool),
    Number(PageNumber),
}

/// A stat's figure: a number, or a string carrying its unit.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StatValue {
    Text(String),
    Number(PageNumber),
}

/// What a callout says of itself: neutral information, a success, a warning or a danger.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalloutTone {
    Info,
    Success,
    Warning,
    Danger,
}

/// Which way a stat's change went, shown as an arrow.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatTrend {
    Up,
    Down,
    Flat,
}

/// Whether a stat's change is good news, bad news or neither, shown as its colour.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatSentiment {
    Positive,
    Negative,
    Neutral,
}

/// Which side of its cells a table column's values sit on, in the reading direction.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnAlign {
    Start,
    End,
}

component_struct! {
    /// Lays its children side by side, each child one column; the columns stack where the panel is
    /// narrow.
    ColumnsLayout { children: Vec<Component>; visible_when: VisibleWhen }
}

component_struct! {
    /// One tab of a [`TabsLayout`]: its label and what it shows.
    TabPane { label: String, children: Vec<Component>; }
}

component_struct! {
    /// Tabs, one of which shows at a time. A tab holds no input field and no submitter.
    TabsLayout { tabs: Vec<TabPane>; visible_when: VisibleWhen }
}

component_struct! {
    /// A thin horizontal rule between blocks, with an optional label on it.
    DividerRule { ; label: String, visible_when: VisibleWhen }
}

component_struct! {
    /// A highlighted note in a tone, its `text` Markdown as a Markdown block's.
    CalloutBlock { text: String; title: String, tone: CalloutTone, visible_when: VisibleWhen }
}

component_struct! {
    /// One pair of a [`KeyValueList`].
    KeyValueItem { key: String, value: CellValue; }
}

component_struct! {
    /// A list of keys, each with its value.
    KeyValueList { items: Vec<KeyValueItem>; visible_when: VisibleWhen }
}

component_struct! {
    /// A figure with its label and an optional change since some earlier figure.
    StatFigure {
        label: String, value: StatValue;
        change: String, trend: StatTrend, sentiment: StatSentiment, visible_when: VisibleWhen
    }
}

component_struct! {
    /// A block of code, highlighted for its `language`, with a button that copies it.
    CodeBlock { code: String; language: String, label: String, visible_when: VisibleWhen }
}

component_struct! {
    /// A unified diff, its added and removed lines marked, with a button that copies it.
    DiffBlock { diff: String; label: String, visible_when: VisibleWhen }
}

component_struct! {
    /// A block of text with a button that copies exactly that text; monospaced unless `monospace`
    /// is false.
    CopyableBlock { text: String; label: String, monospace: bool, visible_when: VisibleWhen }
}

component_struct! {
    /// One column of a [`DataTable`].
    TableColumn { label: String; align: ColumnAlign }
}

component_struct! {
    /// A table of plain values, one cell per column in each row (`null` for an empty one), which
    /// the user can sort by a column and filter unless `sortable` or `filterable` is false.
    DataTable {
        columns: Vec<TableColumn>, rows: Vec<Vec<Option<CellValue>>>;
        caption: String, sortable: bool, filterable: bool, visible_when: VisibleWhen
    }
}
