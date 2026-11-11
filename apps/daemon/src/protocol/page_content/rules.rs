//! What a schema cannot state about a page that has the right shape: names used once, a form with
//! a way to submit it, ranges and defaults that agree, dates that exist, and `visible_when` that
//! names a field the page has outside the component. A page's hidden components are checked like
//! the rest; what is hidden is the user's to decide while filling the form.

use std::collections::BTreeMap;

use super::check::{index, join, PageContentError};
use super::kinds::MOST_COLUMNS;
use super::{
    CheckboxesField, ChoiceButtons, ChoiceOption, Component, DataTable, NumberField, PageContent,
    PageNumber, SliderField, SubmitButton, TemporalField, VisibleEquals, VisibleWhen,
};

/// What an input field's value is, for what `visible_when` may compare it with.
#[derive(Clone, Copy, PartialEq)]
enum Holds {
    Text,
    Number,
    Flag,
}

impl Holds {
    fn article(self) -> &'static str {
        match self {
            Holds::Text => "a string",
            Holds::Number => "a number",
            Holds::Flag => "a boolean",
        }
    }
}

pub(super) struct Input<'a> {
    holds: Holds,
    /// The options of a choice, whose values `visible_when` may name.
    options: Option<&'a [ChoiceOption]>,
}

/// The input fields and the submitters of a page, by name, with the path of each first holder.
#[derive(Default)]
pub(super) struct Names<'a> {
    pub(super) inputs: BTreeMap<&'a str, Input<'a>>,
    input_paths: BTreeMap<&'a str, String>,
    /// A submitter's path, and whether it is a `choices` group, whose name is its own.
    pub(super) submitters: BTreeMap<&'a str, (String, bool)>,
    pub(super) has_submitter: bool,
}

/// What a component is to the page's form.
enum FormPart<'a> {
    Input {
        name: &'a str,
        holds: Holds,
        options: Option<&'a [ChoiceOption]>,
    },
    Submit(&'a SubmitButton),
    Choices(&'a ChoiceButtons),
    /// No part of the form itself, though it may hold components that are.
    Other,
}

impl<'a> FormPart<'a> {
    fn of(component: &'a Component) -> Self {
        let input = |name: &'a String, holds, options| FormPart::Input {
            name: name.as_str(),
            holds,
            options,
        };
        match component {
            Component::Text { component } => input(&component.name, Holds::Text, None),
            Component::Textarea { component } => input(&component.name, Holds::Text, None),
            Component::Number { component } => input(&component.name, Holds::Number, None),
            Component::Date { component }
            | Component::Time { component }
            | Component::Datetime { component } => input(&component.name, Holds::Text, None),
            Component::Radio { component } => {
                input(&component.name, Holds::Text, Some(&component.options))
            }
            Component::Select { component } => {
                input(&component.name, Holds::Text, Some(&component.options))
            }
            Component::Checkboxes { component } => {
                input(&component.name, Holds::Text, Some(&component.options))
            }
            Component::Toggle { component } => input(&component.name, Holds::Flag, None),
            Component::Slider { component } => input(&component.name, Holds::Number, None),
            Component::Submit { component } => FormPart::Submit(component),
            Component::Choices { component } => FormPart::Choices(component),
            Component::Section { .. }
            | Component::Markdown { .. }
            | Component::Columns { .. }
            | Component::Tabs { .. }
            | Component::Divider { .. }
            | Component::Callout { .. }
            | Component::KeyValues { .. }
            | Component::Stat { .. }
            | Component::Code { .. }
            | Component::Diff { .. }
            | Component::Copyable { .. }
            | Component::Table { .. }
            | Component::Chart { .. } => FormPart::Other,
        }
    }
}

fn fail(
    path: impl Into<String>,
    message: impl Into<String>,
    kind: &'static str,
) -> PageContentError {
    PageContentError::new(path, message, Some(kind))
}

pub(super) fn cross_check(content: &PageContent) -> Result<(), PageContentError> {
    let names = gather(content)?;
    check_all(&content.body, "body", &names)?;
    if !names.inputs.is_empty() && !names.has_submitter {
        return Err(PageContentError::new(
            "body",
            "a form needs a `submit` or `choices` to send it with",
            None,
        ));
    }
    Ok(())
}

/// The page's input fields and submitters, refused when a name is used twice.
pub(super) fn gather(content: &PageContent) -> Result<Names<'_>, PageContentError> {
    let mut names = Names::default();
    gather_from(&content.body, "body", &mut names)?;
    for (name, (path, choices)) in &names.submitters {
        if let Some(input_path) = names.input_paths.get(name) {
            return Err(fail(
                join(path, "name"),
                format!("`{name}` is also the name of the input field at `{input_path}`"),
                if *choices { "choices" } else { "submit" },
            ));
        }
    }
    Ok(names)
}

fn gather_from<'a>(
    components: &'a [Component],
    path: &str,
    names: &mut Names<'a>,
) -> Result<(), PageContentError> {
    for (at, component) in components.iter().enumerate() {
        let here = index(path, at);
        match FormPart::of(component) {
            FormPart::Input {
                name,
                holds,
                options,
            } => {
                if let Some(first) = names.input_paths.get(name) {
                    return Err(fail(
                        join(&here, "name"),
                        format!("`{name}` is already the name of the input field at `{first}`"),
                        component.kind(),
                    ));
                }
                names.input_paths.insert(name, here.clone());
                names.inputs.insert(name, Input { holds, options });
            }
            FormPart::Submit(button) => {
                names.has_submitter = true;
                if let Some(name) = &button.name {
                    match names.submitters.get(name.as_str()) {
                        Some((first, true)) => {
                            return Err(fail(
                                join(&here, "name"),
                                format!("`{name}` is the name of the `choices` at `{first}`"),
                                "submit",
                            ))
                        }
                        Some(_) => {}
                        None => {
                            names.submitters.insert(name, (here.clone(), false));
                        }
                    }
                }
            }
            FormPart::Choices(group) => {
                names.has_submitter = true;
                if let Some((first, _)) = names.submitters.get(group.name.as_str()) {
                    return Err(fail(
                        join(&here, "name"),
                        format!(
                            "`{}` is already the name of the submitter at `{first}`",
                            group.name
                        ),
                        "choices",
                    ));
                }
                names.submitters.insert(&group.name, (here.clone(), true));
            }
            FormPart::Other => {}
        }
        for (relative, children) in component.children() {
            gather_from(children, &join(&here, &relative), names)?;
        }
    }
    Ok(())
}

fn check_all(
    components: &[Component],
    path: &str,
    names: &Names<'_>,
) -> Result<(), PageContentError> {
    for (at, component) in components.iter().enumerate() {
        let here = index(path, at);
        check_one(component, &here, names)?;
    }
    Ok(())
}

fn check_one(component: &Component, path: &str, names: &Names<'_>) -> Result<(), PageContentError> {
    let kind = component.kind();
    if let Some(when) = component.visible_when() {
        visible(when, &own_inputs(component), path, kind, names)?;
    }
    match component {
        Component::Text { component: field } => {
            lengths(
                field.min_length,
                field.max_length,
                field.default.as_deref(),
                path,
                kind,
            )?;
            if let Some(pattern) = &field.pattern {
                let regex = compiles(pattern, path)?;
                // A blank default is an empty answer, which the panel never tests against the pattern.
                let default = field
                    .default
                    .as_deref()
                    .filter(|default| !default.trim().is_empty());
                if default.is_some_and(|default| regex.find(default).is_none()) {
                    return Err(fail(
                        join(path, "default"),
                        "does not match its own pattern",
                        kind,
                    ));
                }
            }
        }
        Component::Textarea { component: field } => lengths(
            field.min_length,
            field.max_length,
            field.default.as_deref(),
            path,
            kind,
        )?,
        Component::Number { component: field } => number_range(field, path)?,
        Component::Date { component: field } => {
            temporal(field, path, kind, "YYYY-MM-DD", valid_date)?
        }
        Component::Time { component: field } => temporal(field, path, kind, "HH:MM", valid_time)?,
        Component::Datetime { component: field } => {
            temporal(field, path, kind, "YYYY-MM-DDTHH:MM", valid_datetime)?
        }
        Component::Radio { component: field } => {
            unique_values(&field.options, path, kind)?;
            default_option(field.default.as_deref(), &field.options, path, kind)?
        }
        Component::Select { component: field } => {
            unique_values(&field.options, path, kind)?;
            default_option(field.default.as_deref(), &field.options, path, kind)?
        }
        Component::Checkboxes { component: field } => {
            unique_values(&field.options, path, kind)?;
            selection(field, path)?
        }
        Component::Slider { component: field } => slider_range(field, path)?,
        Component::Submit { component: button } => {
            if button.name.is_some() != button.value.is_some() {
                return Err(fail(
                    path,
                    "`name` and `value` go together: give both or neither",
                    kind,
                ));
            }
        }
        Component::Choices { component: group } => unique_values(&group.options, path, kind)?,
        Component::Columns { component: layout } => {
            if layout.children.len() > MOST_COLUMNS {
                return Err(fail(
                    join(path, "children"),
                    format!(
                        "has {} columns; at most {MOST_COLUMNS} fit side by side",
                        layout.children.len()
                    ),
                    kind,
                ));
            }
        }
        Component::Tabs { .. } => {
            for (relative, children) in component.children() {
                if let Some(inside) = first_form_part(children, &join(path, &relative)) {
                    return Err(fail(
                        inside,
                        "a tab cannot hold an input field or a submitter; put the form outside \
                         the tabs, where all of it can be seen",
                        kind,
                    ));
                }
            }
        }
        Component::Table { component: table } => table_rows(table, path)?,
        Component::Chart { component: chart } => super::chart::check_chart(chart, path)?,
        // Their schemas say all there is to check.
        Component::Section { .. }
        | Component::Markdown { .. }
        | Component::Toggle { .. }
        | Component::Divider { .. }
        | Component::Callout { .. }
        | Component::KeyValues { .. }
        | Component::Stat { .. }
        | Component::Code { .. }
        | Component::Diff { .. }
        | Component::Copyable { .. } => {}
    }
    for (relative, children) in component.children() {
        check_all(children, &join(path, &relative), names)?;
    }
    Ok(())
}

/// The names of the input fields that are `component` or inside it, which its `visible_when`
/// cannot name: a component hidden by its own field could never be shown again.
fn own_inputs(component: &Component) -> Vec<&str> {
    let mut own = Vec::new();
    collect_inputs(std::slice::from_ref(component), &mut own);
    own
}

fn collect_inputs<'a>(components: &'a [Component], into: &mut Vec<&'a str>) {
    for component in components {
        if let FormPart::Input { name, .. } = FormPart::of(component) {
            into.push(name);
        }
        for (_, children) in component.children() {
            collect_inputs(children, into);
        }
    }
}

/// The path of the first input field or submitter among `components` and what they hold.
fn first_form_part(components: &[Component], path: &str) -> Option<String> {
    components.iter().enumerate().find_map(|(at, component)| {
        let here = index(path, at);
        if !matches!(FormPart::of(component), FormPart::Other) {
            return Some(here);
        }
        component
            .children()
            .into_iter()
            .find_map(|(relative, children)| first_form_part(children, &join(&here, &relative)))
    })
}

/// Every row of a table has one cell per column.
fn table_rows(table: &DataTable, path: &str) -> Result<(), PageContentError> {
    let wanted = table.columns.len();
    for (at, row) in table.rows.iter().enumerate() {
        if row.len() != wanted {
            return Err(fail(
                index(&join(path, "rows"), at),
                format!(
                    "has {} cell(s), but the table has {wanted} column(s); give `null` for an empty cell",
                    row.len()
                ),
                "table",
            ));
        }
    }
    Ok(())
}

/// `visible_when` names an input field of the page other than the component's `own`, and compares
/// it with a value that field can hold.
fn visible(
    when: &VisibleWhen,
    own: &[&str],
    path: &str,
    kind: &'static str,
    names: &Names<'_>,
) -> Result<(), PageContentError> {
    let at = join(path, "visible_when");
    let field_at = join(&at, "field");
    if own.contains(&when.field.as_str()) {
        return Err(fail(
            field_at,
            "a component cannot depend on its own field",
            kind,
        ));
    }
    let Some(input) = names.inputs.get(when.field.as_str()) else {
        return Err(fail(
            field_at,
            format!("the page has no input field named `{}`", when.field),
            kind,
        ));
    };
    let equals_at = join(&at, "equals");
    let given = match &when.equals {
        VisibleEquals::Text(_) => Holds::Text,
        VisibleEquals::Number(_) => Holds::Number,
        VisibleEquals::Flag(_) => Holds::Flag,
    };
    if given != input.holds {
        return Err(fail(
            equals_at,
            format!(
                "`{}` holds {}, so `equals` must be {}",
                when.field,
                input.holds.article(),
                input.holds.article()
            ),
            kind,
        ));
    }
    if let (VisibleEquals::Text(wanted), Some(options)) = (&when.equals, input.options) {
        if !options.iter().any(|option| &option.value == wanted) {
            return Err(fail(
                equals_at,
                format!(
                    "`{wanted}` is not the value of any option of `{}`",
                    when.field
                ),
                kind,
            ));
        }
    }
    Ok(())
}

/// A text field's `pattern` compiles as the panel compiles it: as `^(?:pattern)$`, a test of the
/// whole value, with the `v` flag, else `u`, else none, since an agent may write it for any of the
/// three; the first flag set that compiles is the one returned, as the panel settles on it.
fn compiles(pattern: &str, path: &str) -> Result<regress::Regex, PageContentError> {
    let whole = format!("^(?:{pattern})$");
    let mut refusal = None;
    for flags in ["v", "u", ""] {
        match regress::Regex::with_flags(&whole, flags) {
            Ok(regex) => return Ok(regex),
            Err(error) => refusal = Some(error),
        }
    }
    Err(fail(
        join(path, "pattern"),
        format!(
            "is not a JavaScript regular expression: {}",
            refusal.map_or_else(String::new, |error| error.to_string())
        ),
        "text",
    ))
}

fn lengths(
    least: Option<u32>,
    greatest: Option<u32>,
    default: Option<&str>,
    path: &str,
    kind: &'static str,
) -> Result<(), PageContentError> {
    if let (Some(least), Some(greatest)) = (least, greatest) {
        if least > greatest {
            return Err(fail(
                join(path, "min_length"),
                format!("`min_length` {least} is above `max_length` {greatest}"),
                kind,
            ));
        }
    }
    if let Some(default) = default {
        let length = default.chars().count() as u64;
        if least.is_some_and(|least| length < u64::from(least))
            || greatest.is_some_and(|greatest| length > u64::from(greatest))
        {
            return Err(fail(
                join(path, "default"),
                "the default is outside the allowed length",
                kind,
            ));
        }
    }
    Ok(())
}

fn value_of(number: &PageNumber) -> f64 {
    number.0.as_f64().unwrap_or(f64::NAN)
}

fn number_range(field: &NumberField, path: &str) -> Result<(), PageContentError> {
    ranged(
        field.min.as_ref(),
        field.max.as_ref(),
        field.default.as_ref(),
        false,
        path,
        "number",
    )
}

fn slider_range(field: &SliderField, path: &str) -> Result<(), PageContentError> {
    ranged(
        Some(&field.min),
        Some(&field.max),
        field.default.as_ref(),
        true,
        path,
        "slider",
    )
}

/// `min` is not above `max` (below it, where `strict`), and `default` lies between them.
fn ranged(
    least: Option<&PageNumber>,
    greatest: Option<&PageNumber>,
    default: Option<&PageNumber>,
    strict: bool,
    path: &str,
    kind: &'static str,
) -> Result<(), PageContentError> {
    if let (Some(least), Some(greatest)) = (least, greatest) {
        let (least, greatest) = (value_of(least), value_of(greatest));
        if least > greatest || (strict && least == greatest) {
            return Err(fail(
                join(path, "min"),
                if strict {
                    format!("`min` {least} must be below `max` {greatest}")
                } else {
                    format!("`min` {least} is above `max` {greatest}")
                },
                kind,
            ));
        }
    }
    if let Some(default) = default.map(value_of) {
        if least.is_some_and(|least| default < value_of(least))
            || greatest.is_some_and(|greatest| default > value_of(greatest))
        {
            return Err(fail(
                join(path, "default"),
                "the default is outside `min` and `max`",
                kind,
            ));
        }
    }
    Ok(())
}

fn temporal(
    field: &TemporalField,
    path: &str,
    kind: &'static str,
    format: &str,
    well_formed: fn(&str) -> bool,
) -> Result<(), PageContentError> {
    for (name, value) in [
        ("default", &field.default),
        ("min", &field.min),
        ("max", &field.max),
    ] {
        if let Some(value) = value {
            if !well_formed(value) {
                return Err(fail(
                    join(path, name),
                    format!("`{value}` is not a valid {format}"),
                    kind,
                ));
            }
        }
    }
    // The three formats are zero-padded, so their text orders as the moments do.
    if let (Some(least), Some(greatest)) = (&field.min, &field.max) {
        if least > greatest {
            return Err(fail(
                join(path, "min"),
                format!("`min` {least} is after `max` {greatest}"),
                kind,
            ));
        }
    }
    if let Some(default) = &field.default {
        if field.min.as_ref().is_some_and(|least| default < least)
            || field
                .max
                .as_ref()
                .is_some_and(|greatest| default > greatest)
        {
            return Err(fail(
                join(path, "default"),
                "the default is outside `min` and `max`",
                kind,
            ));
        }
    }
    Ok(())
}

/// The number the digits at `range` of `text` spell, when they are all ASCII digits.
fn digits(text: &str, range: std::ops::Range<usize>) -> Option<u32> {
    let part = text.get(range)?;
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

fn valid_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let (Some(year), Some(month), Some(day)) =
        (digits(text, 0..4), digits(text, 5..7), digits(text, 8..10))
    else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

fn valid_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return false;
    }
    matches!(
        (digits(text, 0..2), digits(text, 3..5)),
        (Some(hour), Some(minute)) if hour < 24 && minute < 60
    )
}

fn valid_datetime(text: &str) -> bool {
    text.split_once('T')
        .is_some_and(|(date, time)| valid_date(date) && valid_time(time))
}

fn unique_values(
    options: &[ChoiceOption],
    path: &str,
    kind: &'static str,
) -> Result<(), PageContentError> {
    for (at, option) in options.iter().enumerate() {
        if options[..at]
            .iter()
            .any(|earlier| earlier.value == option.value)
        {
            return Err(fail(
                join(&index(&join(path, "options"), at), "value"),
                format!("the value `{}` is used by an earlier option", option.value),
                kind,
            ));
        }
    }
    Ok(())
}

fn default_option(
    default: Option<&str>,
    options: &[ChoiceOption],
    path: &str,
    kind: &'static str,
) -> Result<(), PageContentError> {
    match default {
        Some(default) if !options.iter().any(|option| option.value == default) => Err(fail(
            join(path, "default"),
            format!("`{default}` is not the value of any option"),
            kind,
        )),
        _ => Ok(()),
    }
}

fn selection(field: &CheckboxesField, path: &str) -> Result<(), PageContentError> {
    let kind = "checkboxes";
    if let Some(least) = field.min_selected {
        if least as usize > field.options.len() {
            return Err(fail(
                join(path, "min_selected"),
                format!(
                    "`min_selected` {least} is more than the {} options",
                    field.options.len()
                ),
                kind,
            ));
        }
        if field.max_selected.is_some_and(|greatest| least > greatest) {
            return Err(fail(
                join(path, "min_selected"),
                "`min_selected` is above `max_selected`",
                kind,
            ));
        }
    }
    if let Some(defaults) = &field.default {
        for (at, value) in defaults.iter().enumerate() {
            let here = index(&join(path, "default"), at);
            if !field.options.iter().any(|option| &option.value == value) {
                return Err(fail(
                    here,
                    format!("`{value}` is not the value of any option"),
                    kind,
                ));
            }
            if defaults[..at].contains(value) {
                return Err(fail(here, format!("`{value}` is listed twice"), kind));
            }
        }
        let chosen = defaults.len();
        if field
            .min_selected
            .is_some_and(|least| chosen < least as usize)
            || field
                .max_selected
                .is_some_and(|greatest| chosen > greatest as usize)
        {
            return Err(fail(
                join(path, "default"),
                "the default chooses fewer or more options than `min_selected` and `max_selected` allow",
                kind,
            ));
        }
    }
    Ok(())
}
