# Report page components

A **report page** is what a session pushes to its report panel (see `docs/product/report-panel.md`): a tree of
**components** drawn from the fixed catalogue below — Markdown, tables, charts, form fields and the rest — which
Octoboard draws with its own interface components. This doc is the catalogue: the page object, each kind of component
and how it is drawn, the checks a page passes when it is pushed, and the guide a session reads them in. How a page is
pushed, paged through and submitted is in `docs/product/report-panel.md`; what a page cannot do, network included, is
in "What a page may contain, and what it cannot do" there.

## The page object

A page is a JSON object with an optional `title` and a `body`, the components shown top to bottom; `show_page` takes
those two properties as its arguments. `title`, when given, is not blank, and is drawn as the page's heading above the
body; `body` holds at least one component. Every component is an object whose `type` names its kind, with that kind's
properties beside it. Any component may also carry `visible_when` (see "Conditional display" below).

```json
{
  "title": "Release check",
  "body": [
    { "type": "markdown", "text": "Two checks failed on **main**." },
    { "type": "radio", "name": "action", "label": "What next?",
      "options": [{ "value": "retry", "label": "Run them again" }, { "value": "skip" }] },
    { "type": "submit", "label": "Send" }
  ]
}
```

The kinds, by what they do:

- **layout**: `section`, `columns`, `tabs`, `divider` (see "Layout" below);
- **content**: `markdown`, `callout`, `key_values`, `stat`, `code`, `diff`, `copyable` (see "Content" below), `table`
  (see "Tables" below) and `chart` (see "Charts" below);
- **input fields**: `text`, `textarea`, `number`, `date`, `time`, `datetime`, `radio`, `select`, `checkboxes`, `toggle`
  and `slider` (see "Input fields" below);
- **submitters**: `submit` and `choices` (see "Submitters" below).

A page with input fields is a **form**; a page with none is a plain report and takes no submission.

## The page guide

A session reads the catalogue with its `page_guide` tool, offered to the same sessions as `show_page` (see "The
console session's tools" and "Project coordinators' tools" in `docs/product/hub-orchestration.md`). It takes a `topic`:

| Topic | Covers |
|---|---|
| `pages` | the page object, `section`, `markdown`, conditional display, and what is drawn how |
| `forms` | the input fields and their checks, `submit` and `choices`, the one-submission rule and what a submission message looks like |
| `report-components` | `columns`, `tabs`, `divider`, `callout`, `key_values`, `stat`, `code`, `diff`, `copyable` and `table` |
| `charts` | `chart` and the part of an ECharts option it takes |

It answers `{ topic, guide, schemas }`: the topic's prose and, by name, the JSON Schema of each kind it covers (`page`
for the page object). Every kind is covered by exactly one topic. A topic it does not have is refused, naming the
topics it has. Like every text aimed at the agents, the guide is English whatever the UI's language.

`show_page`'s own description stays short — what a page is for, that every page is kept until a binding withdraws them,
that only the newest page takes a submission and only once, and to read `page_guide` before the first page — so an
ordinary task carries only the two short descriptions and the catalogue is read when a page is wanted. The console
session's instructions and a coordinator's role description say the same: a page is a tree of components the app draws,
not an HTML document. The guide is a tool of Octoboard's own MCP server, so every agent a session can run reads the same
catalogue.

## Checking a page when it is pushed

The daemon checks a page against the schema of each component's kind before it stores it: the types, the required
properties, no property the kind does not list, no blank string where one is required, and the bounds — a `pattern` up
to **512** characters, a text area's `rows` up to **40**, and every length or selection count a 32-bit unsigned
integer. Beyond the schemas it refuses a page unless:

- names are unique among the input fields, a submitter's name is not an input field's, and a `choices` name is used by
  no other submitter;
- a page with input fields has a submitter, and a `submit` has `name` and `value` together or neither;
- the options of a choice have distinct values, a default is the value of an option (for `checkboxes`, each once, as
  many as `min_selected` and `max_selected` allow), and `min_selected` is at most the number of options and not above
  `max_selected`;
- `min_length` is not above `max_length`, a number's `min` not above its `max` (a slider's below it), a date's or time's
  `min` not after its `max`, and each default is within its field's bounds;
- every date, time and date-and-time is well formed and exists;
- a text field's `pattern` compiles as the panel tests it (see "Checking a form before it is sent" below), and the
  field's `default` matches it;
- `visible_when` names an input field of the page outside the component and a value that field can hold (see
  "Conditional display" below);
- a tab holds no input field and no submitter, at any depth, and `columns` has 2 to 4 children;
- a table row has one cell per column;
- a chart's option stays within the subset and its cross-checks (see "Charts" below).

A page that fails is refused and nothing is stored or announced; the refusal is described in "Pushing a page" in
`docs/product/report-panel.md`. A window shows nothing for a component whose kind it does not know.

## Conditional display

Every component takes `visible_when: { "field": <name>, "equals": <string | number | boolean> }`. The component, with all
it holds, is shown only while the input field named `field` is shown and holds `equals`: for a `checkboxes` field, while
its selection contains it; for a `toggle`, the boolean; for a choice, the value of one of its options. So a part of the
page appears or disappears as the user picks, and a follow-up question is asked only for the answer that needs one.

- The field must be an input field of the page outside the component: not its own, nor one inside the section, columns
  or tabs it is set on.
- A hidden field holds no value for what depends on it, so a component whose condition names a hidden field is hidden
  too.
- **A hidden component's fields are neither checked nor submitted.**

## Layout

- **`section`**: groups `children` (at least one component) under an optional `heading`, drawn a level below the page's
  title.
- **`columns`**: its `children` side by side, one column each, 2 to 4 of them; a column of several components is a
  `section`. The columns stand side by side only once the page is wide enough — **2** from a page **448 px** wide, **3**
  from **576 px**, **4** from **768 px** — and one under another below that, as the panel can be narrow or floating
  whatever the window's width. A column whose content is hidden leaves no empty space; the shown columns share the
  width.
- **`tabs`**: `tabs`, a list of at least two `{ label, children }`, one shown at a time, the first to begin with, under a
  strip of underlined tabs. A tab holds no input field and no submitter, so all of a form stays in view.
- **`divider`**: a thin rule between blocks, usable anywhere a component is, with an optional `label` shown on the rule
  at its start.

## Content

- **`markdown`**: GitHub-flavoured Markdown in `text`, drawn by the application's own Markdown renderer (see
  `docs/product/markdown-documents.md`) in the Tailwind style whatever the Markdown style setting, with these rules for
  a page:
  - raw HTML is not rendered: it is shown as the plain text it was written as;
  - an image is shown only from a `data:` URL; any other stays its description, and nothing is requested for it;
  - a link to an absolute web or mail address opens in the system browser when the user clicks it; a relative link or a
    link to a heading is plain text;
  - its headings start one level below the heading the block sits under.
- **`callout`**: a highlighted note: `text` as Markdown (as a `markdown` block's), an optional `title` drawn as a bold
  line above it, and `tone` — `info` (the default), `success`, `warning` or `danger` — which sets its colour and icon.
  Assistive technology is told the tone in words.
- **`key_values`**: `items`, a list of `{ key, value }` drawn as two columns, the value a string, a number or a boolean.
- **`stat`**: one figure: `label` above `value` (a number, or a string carrying its unit), and optionally `change` (as
  shown, such as "+12%"), `trend` (`up`, `down` or `flat`, drawn as an arrow and told to assistive technology in words)
  and `sentiment` (`positive`, `negative` or `neutral`, the default), which colours the change. Several stats in a
  `columns` make a row of figures.
- **`code`**: `code` highlighted for `language`, with an optional `label`, such as a file name, above it.
- **`diff`**: `diff`, a unified diff as `git diff` prints it, its added and removed lines tinted and its hunk headers
  marked, with an optional `label` above it.
- **`copyable`**: `text` to copy, with an optional `label` above it; drawn monospaced unless `monospace` is false, for
  prose.

**Blocks with text to copy.** A `code`, a `diff` and a `copyable` block are drawn on the code scheme's own surface of the
appearance in force, set apart from the panel, and carry a **copy button** floating over the block's top end corner,
which puts exactly the block's text on the clipboard (see "Copy buttons" in `docs/product/labels-and-tooltips.md`).
Code, a diff and monospaced copyable text keep their lines whole and
scroll sideways rather than wrapping, with the button fixed over the corner; prose copyable text wraps. Copying changes
nothing on the page, so it works on a read-only page too.

## Tables

`table` takes `columns`, a list of `{ label, align? }`, and `rows`, each an array of one plain value per column in the
columns' order: a string, a number, a boolean, or `null` for an empty cell. An optional `caption` names the table and is
drawn above it.

- **Alignment**: `align` is `start` or `end`. Left out, a column whose non-empty cells are all numbers sits at the end,
  and any other at the start.
- **Cells**: a number is formatted for the UI's language and a boolean shown as its word for yes or no (see "Language
  and direction in a page" below). A cell's text wraps at word breaks; a table wider than the page scrolls sideways
  inside its frame.
- **Sorting**: unless `sortable` is false, pressing a column's header sorts the rows by it, ascending, and pressing it
  again reverses the order. Numbers sort by value, text in the order of the UI's language with runs of digits read as
  numbers, and booleans false before true; in a column of mixed values numbers come first, then text, then booleans.
  Empty cells stay last either way, and rows that tie keep the order the session gave them, which is also the order
  before the user sorts.
- **Filtering**: unless `filterable` is false, a search field above the table keeps the rows with a cell containing the
  text typed, as shown or as given, whatever the letter case. While a filter is in force a line says how many of the
  rows are shown, and assistive technology hears it; with no row matching, the table's body is an empty state in the
  compact row's form saying no rows match (see "The compact row" in `docs/product/empty-states.md`).
- On a page with more than one table, each table's filter field and sort buttons are named after its caption, or its
  number among the page's tables, so one table's are told from another's.

Sorting and filtering change only what is shown, never the page, so they work on a read-only page too.

## Input fields

Every input field has `name`, the key its value is submitted under, unique among the page's input fields, and `label`,
and optionally `description`, help text under it, and `required` (every kind but `toggle` and `slider`). A required
field carries no visible mark; it is announced as required to assistive technology. A placeholder is shown only where
the page gives one, except the drop-down's, which has a default of its own.

| Kind | Properties | Drawn as | Submitted as |
|---|---|---|---|
| `text` | `default`, `placeholder`, `pattern`, `min_length`, `max_length` | a single-line field | a string |
| `textarea` | as `text` without `pattern`, plus `rows` (1 to 40, 4 when left out) | a multi-line field | a string |
| `number` | `default`, `min`, `max`, `step` | a single-line field; a comma stands in for the decimal point when the value has no point | a number |
| `date`, `time`, `datetime` | `default`, `min`, `max`, as `YYYY-MM-DD`, `HH:MM` and `YYYY-MM-DDTHH:MM` (local) | the system's own date and time controls | a string in that form |
| `radio` | `options` (`[{ value, label? }]`, values unique; the label is the value when left out), `default` | a group of radio buttons | the chosen value |
| `select` | as `radio`, plus `placeholder` (a localized "choose an option" when left out) | a drop-down | the chosen value |
| `checkboxes` | `options`, `default` (a list of values), `min_selected`, `max_selected`; `required` means at least one | a group of checkboxes | an array of the chosen values, in the options' order |
| `toggle` | `default` (false when left out) | a switch | a boolean |
| `slider` | `min` and `max` (both required, `min` below `max`), `step`, `default` (`min` when left out) | a slider | a number |

What is left out of a submission — an empty optional field, a hidden field — is in "Submitting a form back to the
session" in `docs/product/report-panel.md`.

**Locked fields.** While a page cannot be edited — a history page, a submitted page, or one whose submission is under
way — its fields are drawn as values to read rather than controls to use: text at full strength, a text field's value
still selectable, and nothing that takes a press.

## Submitters

- **`submit`**: a button; `label` is its text, "Submit" in the UI's language when left out. With `name` and `value` it
  also adds `name: value` to the submission, so several `submit` buttons sharing one `name` tell the session which was
  pressed.
- **`choices`**: a row of buttons, one per option of `options`, each of which submits the form with one press and adds
  `name: <the pressed value>`. `name` is required; `label` is a caption over the buttons.

Pressing Enter in a single-line field — text, number, date, time or date and time — presses the page's first shown
`submit` button, as a browser's form does; on a page with only `choices` it does nothing. What a press leads to is in
"Submitting a form back to the session" in `docs/product/report-panel.md`.

## Checking a form before it is sent

A press of a submitter first checks every shown field; while any is wrong, nothing is sent, each wrong field says under
it what is wrong, and keyboard focus goes to the first of them. From then on the fields are checked again as the user
edits them, so a message goes as its field is put right. A value that breaks a `required`, `pattern`, length, bound or
selection limit therefore never reaches the session.

What a field can be told, by kind:

- an empty required field: to enter a value, or for a choice to choose an option, or for `checkboxes` to choose at least
  one;
- `text` and `textarea`: that the value does not match the format the field asks for, that it could not be checked
  against that format (below), or the least or most number of characters;
- `number`: that the value is not a number, the least or most it may be, or the step its values go in, counted from
  `min` (or from 0);
- `date`, `time` and `datetime`: the earliest or latest it may be;
- `checkboxes`: the least or most number of options to choose.

The limits in these messages are formatted for the UI's language. An empty optional field is never wrong, and a text
of only blanks counts as empty.

**A text field's `pattern`** is a JavaScript regular expression the whole value must match: it is tested as
`^(?:pattern)$` with the `v` flag, else the `u` flag, else none, whichever compiles first. It is tested when the user
leaves the field and when the form is submitted, never on each keystroke, and its result is shown on leaving the field
even before any press. Since the pattern is the session's, it is never run on the window's own thread: each test runs in
the background, one at a time, with **100 ms** to finish. A test that does not finish in time counts as not matching,
and the field says its value could not be checked against the expected format. While a press waits for its tests, the
form is locked as it is while a submission is under way.

## Charts

`chart` draws an interactive chart with ECharts inside the page. `option` is an ECharts option, limited to an
allow-listed subset; `height` sets its height, from **160** to **720** px, **320** when left out, and it takes the
page's width. `summary` is a sentence saying what the chart shows: a screen reader reads the chart as one image named
by it, else by the title's text, else by the word for a chart in the UI's language.

**The subset**, every property beyond it refused with its path:

- `series` (required): `line`, `bar`, `pie`, `scatter`, `radar`, `heatmap`, `funnel` and `gauge`, each with its own
  data and shaping options, such as `stack`, `smooth`, `areaStyle`, `label`, `radius` or `roseType`;
- components: `title`, `legend`, `tooltip` (`trigger` `item` or `axis`), `grid`, `xAxis` and `yAxis` (`category`,
  `value`, `time` or `log`), `radar`, `dataZoom` and `visualMap`, each with the properties that shape data and layout.

Left out of the subset, so refused: every colour, font, background and style; every option that writes a string as HTML
— any `formatter`, `rich` text, the HTML tooltip — so every string in a chart is drawn as plain text; everything that
loads or opens an address — image symbols and patterns, `graphic`, a title's `link`; and `dataset`, `toolbox`, `geo`
and maps. Every series names its `type`; a series on axes (`line`, `bar`, `scatter`, `heatmap`) needs `xAxis` and
`yAxis`, and the two it is drawn on in one grid; a `radar` series needs a `radar` with its `indicator`; a `heatmap`
needs a `visualMap` and two `category` axes; and every `gridIndex`, `xAxisIndex`, `yAxisIndex`, `radarIndex` and
`seriesIndex` names one the option has.

**The colours are the app's**, from the appearance in force, never the option's, since a session cannot know which
appearance its chart will be read in: the series take a fixed palette of eight hues in their order, never by rank,
each hue stepped for the light and for the dark appearance and chosen to stay apart under colour-vision deficiency; a
`visualMap` shades with a one-hue ramp from near the surface to strong; text, axes and grid lines take the interface's
own colours, and the text the system font. The tooltip is drawn inside the chart, as text, on the colours of a floating
surface. The chart is redrawn when the appearance changes, and is not animated when the system asks for reduced
motion.

**The layout is the app's too**: the title sits at the top, a horizontal legend right under it, scrolling rather than
wrapping when its names do not fit, and the plotting area below both, so none lies on another at any width. In a chart
narrower than **480 px** a legend set at the side moves under the title as well, and a pie or a funnel is centred below
it.

**Interactions** are ECharts' own — a tooltip on hover, turning series on and off from the legend and, where the option
has a `dataZoom`, zooming — and they are the pointer's alone; the numbers a keyboard user needs belong in a table or a
key-value block beside the chart. The chart library loads the first time a page shows a chart; meanwhile, and where an
option cannot be drawn, the chart's box keeps its height, and a chart that cannot be drawn says so in it.

## Language and direction in a page

The words of a page — its title, headings, labels, options, Markdown and cells — are the session's and are shown as
written. Everything Octoboard supplies follows the UI's language (see "What follows the language" in
`docs/product/language.md`): a submit button's default text, the drop-down's default placeholder, the messages of the
form checks, the words for yes and no in a cell, the table's filter, count and empty state, the copy button's name and
the states the panel shows. Numbers in a key-value list, a stat or a table are formatted for the UI's language.

The page's layout follows the UI's direction and is mirrored under a right-to-left language (see
`docs/product/right-to-left-layout.md`). Each block of the session's text takes the direction of its own text, while its
alignment follows the UI, so a left-to-right paragraph in a right-to-left page reads from the left yet lines up with the
headings around it. Code and a diff always read left to right, with their copy button at their right-hand end.
