//! The guide a session reads up before it pushes a page (`page_guide`): the topics, each with its
//! prose and the schemas of the kinds of component it covers. A later kind of component adds its
//! schema to `crate::protocol` and a topic here, or its name to the `schemas` of one.

use serde_json::{json, Map, Value};

use crate::protocol::schema_named;

/// One topic of the guide.
pub(crate) struct Topic {
    pub name: &'static str,
    /// What the topic covers, as the tool's `topic` argument says.
    pub summary: &'static str,
    pub guide: &'static str,
    /// The kinds of component, and `page` for the page object, whose schemas it hands over.
    pub schemas: &'static [&'static str],
}

/// Every topic, in the order the tool lists them.
pub(crate) const TOPICS: &[Topic] = &[
    Topic {
        name: "pages",
        summary: "the page object, sections, Markdown, conditional display and what renders how",
        guide: PAGES,
        schemas: &["page", "section", "markdown"],
    },
    Topic {
        name: "forms",
        summary: "input fields, validation, submit buttons and choices, and what a submission \
                  looks like",
        guide: FORMS,
        schemas: &[
            "text",
            "textarea",
            "number",
            "date",
            "time",
            "datetime",
            "radio",
            "select",
            "checkboxes",
            "toggle",
            "slider",
            "submit",
            "choices",
        ],
    },
    Topic {
        name: "report-components",
        summary:
            "columns, tabs, dividers, callouts, key-value lists, stats, code, diffs, copyable text \
                  and tables for a visual report",
        guide: REPORT_COMPONENTS,
        schemas: &[
            "columns",
            "tabs",
            "divider",
            "callout",
            "key_values",
            "stat",
            "code",
            "diff",
            "copyable",
            "table",
        ],
    },
    Topic {
        name: "charts",
        summary: "the chart component: which part of an ECharts option it takes, and how it is \
                  drawn",
        guide: CHARTS,
        schemas: &["chart"],
    },
];

const PAGES: &str = "\
# Pages

A page is a JSON object with an optional `title` and a `body`, the components shown top to bottom. \
`show_page` takes those two properties as its arguments. Every component is an object whose `type` \
names its kind and which carries that kind's properties; a property the kind does not list is \
refused. A page that breaks the schemas below is refused with the JSON path of the failing part \
(such as `body[2].children[0].options[1].value`) and the schema of that part's kind, and nothing is \
stored: correct the page and push it again.

## Kinds in this topic

- `section`: groups `children` (at least one component) under an optional `heading`.
- `markdown`: GitHub-flavoured Markdown in `text`: headings, emphasis, lists, tables, code, task \
lists, links, math.

The input fields and the buttons that submit them are in the `forms` topic. Layout (columns, tabs, \
dividers) and the other blocks of a visual report (callouts, key-value lists, stats, code, diffs, \
copyable text, tables) are in the `report-components` topic. A chart is in the `charts` topic.

## Conditional display

Every component takes `visible_when: { \"field\": <name of an input field on this page>, \
\"equals\": <string | number | boolean> }`. The component, with all it contains, is shown only \
while that field holds `equals` (a `checkboxes` field: while its selection contains it; a `toggle`: \
the boolean). Use it to ask a follow-up only for the answer that needs one. A hidden component's \
fields are neither checked nor submitted. The field must be an input of the page outside the \
component (not its own, nor one inside a section, columns or tabs it is set on), and `equals` must \
be a value the field can hold.

## What renders how

The app draws the page with its own components, in the user's appearance and language, so there is \
no styling to give and no HTML to write: raw HTML in Markdown is shown as plain text. Nothing is loaded \
from the network: an image shows only from a `data:` URL, and a link opens in the user's system \
browser. Keep a page to what is better shown than typed into the terminal: a table, a comparison, \
a report, a set of choices, a question with a form.

## Lifetime

Every page is kept, and the user can page back through earlier ones, until this session is bound to \
a project session or handed to one, which withdraws them all (`delete_pages` withdraws them on \
request, `list_pages` reads them back). Only the newest page accepts a submission, and only once.";

const FORMS: &str = "\
# Forms

A page with input fields is a form, and it needs a submitter: a `submit` or a `choices`. Fields and \
submitters can sit anywhere in the body, inside sections too. A page with neither is a plain report.

## Input fields

Each has `name` (the key its value is submitted under, unique among the page's input fields), \
`label`, and optionally `description` (help text) and `required` (every kind but `toggle` and \
`slider`). Defaults, bounds and lengths are checked when the page is pushed: a minimum above its \
maximum, a default outside its range or not among the options, or a date that does not exist is \
refused.

- `text`: a line of text; `default`, `placeholder`, `pattern` (a JavaScript regular expression the \
whole value must match, at most 512 characters; one that does not compile is refused when the page \
is pushed, as is a `default` that does not match it), `min_length`, `max_length`. Submitted as a string.
- `textarea`: several lines; as `text` without `pattern`, plus `rows`.
- `number`: `default`, `min`, `max`, `step`. Submitted as a number.
- `date` (`YYYY-MM-DD`), `time` (`HH:MM`) and `datetime` (`YYYY-MM-DDTHH:MM`, local): `default`, \
`min`, `max`. Submitted as a string.
- `radio`: one choice from `options` (`[{ \"value\", \"label\"? }]`, values unique), `default` a \
value. Submitted as the value.
- `select`: the same as a drop-down, with a `placeholder`.
- `checkboxes`: several choices from `options`, `default` a list of values, `min_selected`, \
`max_selected`; `required` means at least one. Submitted as an array of the values, in the \
options' order.
- `toggle`: an on/off switch, `default` a boolean. Submitted as a boolean.
- `slider`: `min` and `max` (both required, `min` below `max`), `step`, `default`. Submitted as a \
number.

The user is told what is wrong with a field before the form is sent, so a value that breaks a \
`required`, `pattern`, length, bound or selection limit never reaches you.

## Submitters

- `submit`: a button; `label` is its text (\"Submit\" when left out). With `name` and `value` it also \
adds `name: value` to the submission, so several `submit`s sharing one `name` tell you which was \
pressed.
- `choices`: a row of buttons, one per option of `options`, each of which submits the form with one \
press. `name` is required and adds `name: <the pressed value>`; `label` is a caption over the buttons.

A submitter's `name` must not be an input field's name, and a `choices` name is used by no other \
submitter.

## One submission

A form is submitted once. The moment the user presses a submitter the page is marked submitted in \
every window and shows what was sent, read-only, however often or from wherever it is pressed \
again; you get exactly one message. If the message certainly could not be delivered, the mark is \
taken off and the user can submit again. Only the newest page accepts a submission, so push a fresh \
form rather than expecting an answer from an old page. `list_pages` with a `page` shows the \
page's `submission`: `null` until it is submitted.

## What you receive

A message with the page's id, then a blank line and one `key: value` line per submitted field, in \
the page's order, a multiple choice joined by \", \", a boolean as `true` or `false`. A hidden field \
is not in it. For a page with fields `name`, `topics` (checkboxes), `subscribe` (toggle) and a \
`choices` named `action`:

    Report panel form submission — page 1f0e…

    name: Ada
    topics: rust, sqlite
    subscribe: true
    action: approve";

const REPORT_COMPONENTS: &str = "\
# Report components

What a visual report shows beyond Markdown. They hold no input, so a page of them alone is a plain \
report; they sit anywhere in the body beside a form, and every one takes `visible_when` as in the \
`pages` topic. Each is drawn in the app's own style and colours, in light and dark alike.

## Layout

- `columns`: its `children` side by side, each child one column, 2 to 4 of them; a column of \
several components is a `section`. Where the panel is narrow the columns stack one above the other, \
so do not rely on them being beside each other.
- `tabs`: `tabs` is a list (at least two) of `{ \"label\", \"children\" }`, one shown at a time, the \
first at the start. A tab holds no input field and no submitter: keep a form outside the tabs, \
where all of it can be seen.
- `divider`: a thin rule between blocks, usable anywhere a component is; an optional `label` is \
shown on the rule.

## Content

- `callout`: a highlighted note; `text` is Markdown, `title` an optional bold line above it, and \
`tone` one of `info` (the default), `success`, `warning`, `danger`, which sets its colour and icon.
- `key_values`: `items`, a list of `{ \"key\", \"value\" }`, the value a string, number or boolean.
- `stat`: one figure; `label`, `value` (a number, or a string carrying its unit, such as \
`\"4.2 s\"`), and optionally `change` (as shown, such as `\"+12%\"`), `trend` (`up`, `down`, `flat`, \
drawn as an arrow) and `sentiment` (`positive`, `negative`, `neutral`, the default, its colour). \
Several stats in a `columns` make a row of figures.
- `code`: `code` highlighted for `language`, with an optional `label` such as a file name, and a \
button that copies the code.
- `diff`: `diff`, a unified diff as `git diff` prints it, its added and removed lines marked, with an \
optional `label` and a button that copies it.
- `copyable`: `text` with a button that copies exactly that text and tells the user it was \
copied; an optional `label` above it, and `monospace` (true by default; false for prose). Use it \
for something the user is to paste elsewhere: a command, an address, a token.

## Tables

`table` takes `columns`, a list of `{ \"label\", \"align\"? }`, and `rows`, each an array of one \
plain value per column in the columns' order: a string, a number, a boolean, or `null` for an empty \
cell. A row with a cell too many or too few is refused. `align` is `start` or `end`; left out, a \
column of numbers sits at the end. `caption` names the table above it. The user can sort it by any \
column (numbers by value, text in the user's language's order) and filter its rows by the text in \
them; `sortable: false` or `filterable: false` turns either off. Send the rows in the order you want \
them read first: that order is kept until the user sorts.";

const CHARTS: &str = "\
# Charts

A `chart` draws an interactive chart with ECharts, inside the page: the user can hover for a \
tooltip, turn series on and off from the legend and, where you add a `dataZoom`, zoom. Its \
`option` is an ECharts option, but only the part of one that the schema below lists: any other \
property is refused with its path, and nothing is stored. `height` sets the height in pixels \
(160 to 720, 320 when left out); the width is the page's. Give `summary`, a sentence saying what \
the chart shows: a screen reader announces it in place of the drawing (the title's text when left \
out).

## The subset

- `series` (required): `line`, `bar`, `pie`, `scatter`, `radar`, `heatmap`, `funnel` and `gauge`, \
each with its `name` and `data`, and options such as `stack`, `smooth`, `areaStyle`, `label`, \
`radius` or `roseType`.
- `xAxis` and `yAxis` for `line`, `bar`, `scatter` and `heatmap`: `category` (names in `data`), \
`value`, `time` or `log`. A horizontal bar chart puts the `category` axis on `yAxis`.
- `radar` for `radar` series (its `indicator` spokes), `visualMap` for a `heatmap`, `grid`, \
`title`, `legend`, `tooltip` (`trigger`: `item` or `axis`) and `dataZoom`.

Not in the subset, so refused: every colour, font, background and style (the app draws a chart in \
its own palette, in light and dark, so a series' colour follows its order); every `formatter`, \
`rich` text and HTML tooltip (nothing in a chart renders as HTML: every string you give is drawn as \
plain text, so give the words as you want them read); anything that loads or opens an address \
(images, `link`); `dataset`, `toolbox`, `graphic`, `geo` and maps. Put numbers in the units you want \
shown, and name the unit in the axis `name` or the series `name`.

Every series names its `type`. A series on axes needs both `xAxis` and `yAxis`, and the two it is \
drawn on (`xAxisIndex` and `yAxisIndex`, the first of each when left out) must be in the same \
`grid`. A `radar` series needs `radar`, with its `indicator`. A `heatmap` needs a `visualMap`, and \
both its axes must be `\"type\": \"category\"`. Every `xAxisIndex`, `yAxisIndex`, `radarIndex`, \
`gridIndex` and `seriesIndex` must name one the option has. Otherwise the page is refused.

## Example

    { \"type\": \"chart\", \"summary\": \"Weekly builds: passes rose from 12 to 30 while failures stayed under 5.\",
      \"option\": {
        \"title\": { \"text\": \"Builds per week\" },
        \"tooltip\": { \"trigger\": \"axis\" },
        \"legend\": {},
        \"xAxis\": { \"type\": \"category\", \"data\": [\"W1\", \"W2\", \"W3\", \"W4\"] },
        \"yAxis\": { \"type\": \"value\", \"name\": \"Builds\" },
        \"series\": [
          { \"type\": \"bar\", \"name\": \"Passed\", \"stack\": \"all\", \"data\": [12, 18, 25, 30] },
          { \"type\": \"bar\", \"name\": \"Failed\", \"stack\": \"all\", \"data\": [4, 3, 5, 2] }
        ] } }";

/// The topic called `name`.
pub(crate) fn topic(name: &str) -> Option<&'static Topic> {
    TOPICS.iter().find(|topic| topic.name == name)
}

/// What `page_guide` answers for `topic`: its prose and the schema of each kind it covers, by name.
pub(crate) fn read(topic: &Topic) -> Value {
    let schemas: Map<String, Value> = topic
        .schemas
        .iter()
        .filter_map(|name| schema_named(name).map(|schema| ((*name).to_string(), schema)))
        .collect();
    json!({ "topic": topic.name, "guide": topic.guide, "schemas": schemas })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::kind_names;
    use crate::protocol::SessionKind;
    use crate::session_tools::catalogue::tool_by_name;

    /// Every kind of component, and the page object, has its schema in exactly one topic, and each
    /// topic's prose names the kinds whose schemas it hands over, so a new kind cannot be left out
    /// of the guide.
    #[test]
    fn every_kind_is_covered_by_one_topic() {
        for name in kind_names().chain(["page"]) {
            let covering: Vec<&str> = TOPICS
                .iter()
                .filter(|topic| topic.schemas.contains(&name))
                .map(|topic| topic.name)
                .collect();
            assert_eq!(covering.len(), 1, "{name}: {covering:?}");
        }
        for topic in TOPICS {
            for name in topic.schemas.iter().filter(|name| **name != "page") {
                assert!(
                    topic.guide.contains(&format!("`{name}`")),
                    "the `{}` topic does not describe `{name}`",
                    topic.name
                );
            }
        }
    }

    /// A topic answers with its prose and the schema of each kind it covers, by name.
    #[test]
    fn a_topic_answers_with_its_guide_and_schemas() {
        let pages = read(topic("pages").unwrap());
        assert_eq!(pages["topic"], "pages");
        assert!(pages["guide"].as_str().unwrap().contains("visible_when"));
        let schemas = pages["schemas"].as_object().unwrap();
        assert_eq!(
            schemas.keys().collect::<Vec<_>>(),
            ["page", "section", "markdown"]
        );
        assert_eq!(
            schemas["markdown"]["properties"]["type"]["const"],
            "markdown"
        );

        let forms = read(topic("forms").unwrap());
        let guide = forms["guide"].as_str().unwrap();
        assert!(guide.contains("One submission"));
        assert!(guide.contains("topics: rust, sqlite"));
        assert_eq!(forms["schemas"].as_object().unwrap().len(), 13);

        let report = read(topic("report-components").unwrap());
        assert_eq!(report["schemas"].as_object().unwrap().len(), 10);
        assert_eq!(
            report["schemas"]["table"]["properties"]["type"]["const"],
            "table"
        );

        let charts = read(topic("charts").unwrap());
        assert!(charts["guide"].as_str().unwrap().contains("formatter"));
        assert_eq!(
            charts["schemas"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            ["chart"]
        );
    }

    /// The tool's description and its `topic` argument name every topic.
    #[test]
    fn the_tool_names_every_topic() {
        let tool = tool_by_name(SessionKind::Console, false, "page_guide").unwrap();
        let schema = (tool.schema)();
        let listed = schema["properties"]["topic"]["enum"].as_array().unwrap();
        assert_eq!(listed.len(), TOPICS.len());
        for topic in TOPICS {
            assert!(tool.description.contains(topic.name), "{}", topic.name);
            assert!(listed.contains(&json!(topic.name)));
        }
        assert_eq!(schema["required"], json!(["topic"]));
    }
}
