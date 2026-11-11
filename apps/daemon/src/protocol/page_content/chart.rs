//! The chart component: an ECharts option, limited to a subset the app can draw safely, with the
//! schema that states the subset and the cross-checks it cannot state.
//!
//! The subset is an allow-list. It keeps what describes data and the interactions ECharts draws
//! itself (a tooltip on hover, toggling series from the legend, zooming), and leaves out three
//! groups of options: those that make ECharts put a string into the window as HTML (a `formatter`
//! of any kind, `rich` text, the tooltip's `renderMode` and `extraCssText`, the toolbox's data view),
//! those that load or open an address (an `image://` symbol, a pattern or `graphic` image, a title's
//! `link`, the toolbox's image download), and every colour, font and background, which the app
//! supplies from the user's appearance so a chart reads in light and dark alike. The renderer in the
//! UI (`packages/ui/src/report/page/chart/`) forces the same rules again on what it draws.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::check::{index, join, PageContentError};
use super::kinds::{flag, kind_schema, non_blank, number_property, string};

/// A chart: an ECharts option within the subset of [`chart`], drawn in the user's appearance.
#[cfg_attr(test, derive(ts_rs::TS, schemars::JsonSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    /// The ECharts option, as the schema of the `chart` kind allows it.
    #[cfg_attr(
        test,
        ts(type = "Record<string, unknown>"),
        schemars(with = "serde_json::Map<String, Value>")
    )]
    pub option: Value,
    /// The height of the drawing in pixels; the width is the page's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub height: Option<u32>,
    /// What the chart shows, in words: its accessible name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub visible_when: Option<super::VisibleWhen>,
}

/// The series types the subset has.
const SERIES_TYPES: &[&str] = &[
    "line", "bar", "pie", "scatter", "radar", "heatmap", "funnel", "gauge",
];

/// The series types drawn on a grid's `xAxis` and `yAxis`.
const CARTESIAN: &[&str] = &["line", "bar", "scatter", "heatmap"];

/// The least and greatest `height`.
const HEIGHT_RANGE: (u32, u32) = (160, 720);

fn one_of(values: &[&str], description: &str) -> Value {
    json!({ "enum": values, "description": description })
}

fn position(description: &str) -> Value {
    json!({
        "type": ["number", "string"],
        "description": format!("{description} Pixels, a percentage such as \"10%\", or a word such as \"center\"."),
    })
}

fn axis_index(description: &str) -> Value {
    json!({ "type": "integer", "minimum": 0, "description": description })
}

fn axis_indexes(description: &str) -> Value {
    json!({
        "type": ["integer", "array"],
        "minimum": 0,
        "items": { "type": "integer", "minimum": 0 },
        "description": description,
    })
}

/// An object with these properties and no other.
fn object(description: &str, properties: Value) -> Value {
    json!({
        "type": "object",
        "description": description,
        "properties": properties,
        "additionalProperties": false,
    })
}

/// A component ECharts takes as one object or as an array of them.
fn one_or_more(description: &str, properties: Value) -> Value {
    json!({
        "type": ["object", "array"],
        "description": format!("{description} One object, or an array of them."),
        "properties": properties.clone(),
        "additionalProperties": false,
        "minItems": 1,
        "items": object(description, properties),
    })
}

fn edges() -> [(&'static str, Value); 4] {
    [
        ("left", position("Distance from the left.")),
        ("top", position("Distance from the top.")),
        ("right", position("Distance from the right.")),
        ("bottom", position("Distance from the bottom.")),
    ]
}

/// `properties` with the four edge positions added.
fn placed(properties: Value) -> Value {
    let mut properties = properties;
    if let Value::Object(map) = &mut properties {
        for (name, schema) in edges() {
            map.insert(name.to_string(), schema);
        }
    }
    properties
}

fn scalar() -> Value {
    json!({ "type": ["number", "string", "null"] })
}

fn data_item() -> Value {
    json!({
        "type": ["number", "string", "null", "array", "object"],
        "description": "A value; an array of values (such as [x, y] or [x, y, value]); or \
                        { \"name\", \"value\" } where `value` is a value or an array of them. \
                        \"-\" or null is a missing value.",
        "items": scalar(),
        "properties": {
            "name": string("The item's name, as the legend, a pie's label and the tooltip show it."),
            "value": {
                "type": ["number", "string", "null", "array"],
                "items": scalar(),
                "description": "The item's value, or its values.",
            },
        },
        "additionalProperties": false,
    })
}

fn axis() -> Value {
    json!({
        "show": flag("Whether the axis is drawn."),
        "type": one_of(&["category", "value", "time", "log"], "The kind of axis; `category` takes its names from `data`."),
        "name": string("The axis' name."),
        "nameLocation": one_of(&["start", "middle", "center", "end"], "Where the name sits."),
        "nameGap": number_property("The gap between the name and the axis line, in pixels."),
        "data": {
            "type": "array",
            "items": { "type": ["string", "number"] },
            "description": "The category names of a `category` axis, in order.",
        },
        "min": number_property("The axis' least value."),
        "max": number_property("The axis' greatest value."),
        "scale": flag("Whether a `value` axis may leave out zero."),
        "logBase": json!({ "type": "number", "exclusiveMinimum": 1, "description": "The base of a `log` axis." }),
        "boundaryGap": flag("Whether a `category` axis leaves half a band at each end."),
        "inverse": flag("Whether the axis runs the other way."),
        "position": one_of(&["top", "bottom", "left", "right"], "The side of the grid the axis is on."),
        "gridIndex": axis_index("The `grid` the axis is in."),
        "splitLine": object("The lines across the grid at each tick.", json!({ "show": flag("Whether they are drawn.") })),
        "axisLabel": object("The tick labels.", json!({
            "show": flag("Whether they are drawn."),
            "rotate": number_property("Their rotation in degrees, -90 to 90."),
            "interval": json!({ "type": "integer", "minimum": 0, "description": "How many labels to skip between two shown." }),
        })),
    })
}

fn series() -> Value {
    let mut schema = object(
        "One series of data. Properties a series type does not use are ignored.",
        json!({
            "type": one_of(SERIES_TYPES, "The series type."),
            "name": string("The series' name, as the legend and the tooltip show it."),
            "data": {
                "type": "array",
                "items": data_item(),
                "description": "The series' data: on a `category` axis one value per category; \
                                otherwise [x, y] pairs, { name, value } items for `pie`, `funnel` \
                                and `gauge`, { name, value: [...] } for `radar`, [x, y, value] for \
                                `heatmap`.",
            },
            "stack": string("Series with the same `stack` are stacked (`line`, `bar`)."),
            "smooth": json!({ "type": ["boolean", "number"], "description": "Whether a `line` is smoothed, or how much (0 to 1)." }),
            "step": json!({ "enum": [true, false, "start", "middle", "end"], "description": "A stepped `line`." }),
            "areaStyle": object("Fills the area under a `line` (stacked: between lines).", json!({
                "opacity": json!({ "type": "number", "minimum": 0, "maximum": 1, "description": "The fill's opacity." }),
            })),
            "showSymbol": flag("Whether a `line` marks each point."),
            "symbol": one_of(&["circle", "rect", "roundRect", "triangle", "diamond", "pin", "arrow", "none"], "The mark of each point."),
            "symbolSize": json!({ "type": "number", "minimum": 0, "description": "The size of each point's mark, in pixels." }),
            "barWidth": position("The width of a bar."),
            "barMaxWidth": position("The greatest width of a bar."),
            "barGap": string("The gap between bars of different series in one category, such as \"30%\"."),
            "label": object("The labels on the data.", json!({
                "show": flag("Whether they are drawn."),
                "position": one_of(&["top", "left", "right", "bottom", "inside", "insideTop", "insideLeft", "insideRight", "insideBottom", "outside", "center"], "Where they sit."),
            })),
            "labelLine": object("The lines to a `pie`'s or `funnel`'s outside labels.", json!({ "show": flag("Whether they are drawn.") })),
            "emphasis": object("What hovering does.", json!({
                "focus": one_of(&["none", "self", "series"], "What stays prominent while the rest fades."),
                "disabled": flag("Whether hovering highlights nothing."),
            })),
            "radius": json!({
                "type": ["number", "string", "array"],
                "items": { "type": ["number", "string"] },
                "description": "A `pie`'s or `gauge`'s radius, or [inner, outer] for a ring.",
            }),
            "center": json!({
                "type": "array",
                "items": { "type": ["number", "string"] },
                "description": "A `pie`'s or `gauge`'s centre, [x, y].",
            }),
            "roseType": one_of(&["radius", "area"], "Draws a `pie` as a rose."),
            "startAngle": number_property("A `pie`'s or `gauge`'s starting angle in degrees."),
            "endAngle": number_property("A `gauge`'s ending angle in degrees."),
            "min": number_property("A `gauge`'s or `funnel`'s least value."),
            "max": number_property("A `gauge`'s or `funnel`'s greatest value."),
            "sort": one_of(&["ascending", "descending", "none"], "The order of a `funnel`."),
            "gap": json!({ "type": "number", "minimum": 0, "description": "The gap between a `funnel`'s stages, in pixels." }),
            "progress": object("A `gauge`'s progress arc.", json!({ "show": flag("Whether it is drawn.") })),
            "xAxisIndex": axis_index("The `xAxis` of a `line`, `bar`, `scatter` or `heatmap`."),
            "yAxisIndex": axis_index("The `yAxis` of a `line`, `bar`, `scatter` or `heatmap`."),
            "radarIndex": axis_index("The `radar` of a `radar` series."),
        }),
    );
    schema["required"] = json!(["type"]);
    schema
}

/// The `radar` coordinate system, one or several, each of which needs its spokes.
fn radar() -> Value {
    let mut schema = one_or_more(
        "The coordinate system of `radar` series.",
        json!({
            "indicator": {
                "type": "array",
                "minItems": 1,
                "description": "Its spokes.",
                "items": object("A spoke.", json!({
                    "name": string("The spoke's name."),
                    "min": number_property("Its least value."),
                    "max": number_property("Its greatest value."),
                })),
            },
            "shape": one_of(&["polygon", "circle"], "Its shape."),
            "center": { "type": "array", "items": { "type": ["number", "string"] }, "description": "Its centre, [x, y]." },
            "radius": { "type": ["number", "string"], "description": "Its radius." },
            "startAngle": number_property("The angle of the first spoke, in degrees."),
        }),
    );
    schema["required"] = json!(["indicator"]);
    schema["items"]["required"] = json!(["indicator"]);
    schema
}

fn option() -> Value {
    let mut schema = object(
        "The ECharts option, limited to the properties listed here. Colours, fonts and backgrounds \
         are the app's, from the user's appearance, and nothing in a chart renders as HTML or \
         loads an address.",
        json!({
            "title": object("The chart's title.", placed(json!({
                "text": string("The title."),
                "subtext": string("A line under the title."),
                "textAlign": one_of(&["left", "center", "right"], "How the title is aligned."),
            }))),
            "legend": object("The legend, from which the user turns series on and off.", placed(json!({
                "show": flag("Whether it is drawn."),
                "type": one_of(&["plain", "scroll"], "`scroll` pages through a long legend."),
                "orient": one_of(&["horizontal", "vertical"], "Which way it runs."),
                "data": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "The names it lists; every series' (or a pie's items') when left out.",
                },
                "selectedMode": json!({ "enum": [true, false, "single", "multiple"], "description": "Whether, and how, the user can turn series on and off." }),
            }))),
            "tooltip": object("The tooltip shown on hover.", json!({
                "show": flag("Whether it is shown."),
                "trigger": one_of(&["item", "axis", "none"], "`item`: for the mark under the pointer; `axis`: for every series at the pointer's category."),
                "axisPointer": object("The guide drawn at the pointer with `trigger: \"axis\"`.", json!({
                    "type": one_of(&["line", "shadow", "cross", "none"], "Its kind."),
                })),
            })),
            "grid": one_or_more("The plotting area of `xAxis` and `yAxis`.", placed(json!({}))),
            "xAxis": one_or_more("The horizontal axis.", axis()),
            "yAxis": one_or_more("The vertical axis.", axis()),
            "radar": radar(),
            "dataZoom": one_or_more("Zooming into part of an axis: `inside` by wheel and drag, `slider` with a slider.", json!({
                "type": one_of(&["inside", "slider"], "How the user zooms."),
                "xAxisIndex": axis_indexes("The `xAxis` (or axes) it zooms."),
                "yAxisIndex": axis_indexes("The `yAxis` (or axes) it zooms."),
                "start": json!({ "type": "number", "minimum": 0, "maximum": 100, "description": "Where the shown range starts, as a percentage." }),
                "end": json!({ "type": "number", "minimum": 0, "maximum": 100, "description": "Where the shown range ends, as a percentage." }),
                "orient": one_of(&["horizontal", "vertical"], "Which way a slider runs."),
            })),
            "visualMap": one_or_more("Maps values to shades of the app's sequential colour, as a `heatmap` needs.", placed(json!({
                "type": one_of(&["continuous", "piecewise"], "A gradient, or bands."),
                "min": number_property("The least value of the gradient."),
                "max": number_property("The greatest value of the gradient."),
                "calculable": flag("Whether the user can drag the range."),
                "orient": one_of(&["horizontal", "vertical"], "Which way it runs."),
                "dimension": axis_index("The index of the value in each data item it maps."),
                "seriesIndex": axis_indexes("The series it maps; all when left out."),
                "splitNumber": json!({ "type": "integer", "minimum": 1, "description": "The number of bands a `piecewise` map makes of [min, max]." }),
                "text": { "type": "array", "items": { "type": "string" }, "description": "The words at the high and the low end." },
                "pieces": {
                    "type": "array",
                    "description": "The bands of a `piecewise` map.",
                    "items": object("A band.", json!({
                        "min": number_property("Its least value."),
                        "max": number_property("Its greatest value."),
                        "lt": number_property("Values below this."),
                        "lte": number_property("Values up to this."),
                        "gt": number_property("Values above this."),
                        "gte": number_property("Values from this."),
                        "value": number_property("Exactly this value."),
                        "label": string("Its name."),
                    })),
                },
            }))),
            "series": {
                "type": "array",
                "minItems": 1,
                "description": "The chart's series.",
                "items": series(),
            },
        }),
    );
    schema["required"] = json!(["series"]);
    schema
}

pub(super) fn chart() -> Value {
    kind_schema(
        "chart",
        "An interactive chart drawn by ECharts from `option`, a subset of an ECharts option, in \
         the app's colours.",
        json!({
            "option": option(),
            "height": json!({
                "type": "integer",
                "minimum": HEIGHT_RANGE.0,
                "maximum": HEIGHT_RANGE.1,
                "description": "The height in pixels; 320 when left out. The width is the page's.",
            }),
            "summary": non_blank("What the chart shows, in a sentence: what a screen reader announces. The title's text when left out."),
        }),
        &["option"],
    )
}

/// How many objects the option has under `key`: none, one, or the array's length.
fn count(option: &Value, key: &str) -> usize {
    match option.get(key) {
        None => 0,
        Some(Value::Array(items)) => items.len(),
        Some(_) => 1,
    }
}

/// Each object under `key` with its path: the object itself, or each item of the array.
fn each<'a>(option: &'a Value, key: &str, path: &str) -> Vec<(String, &'a Value)> {
    let at = join(path, key);
    match option.get(key) {
        None => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .enumerate()
            .map(|(i, item)| (index(&at, i), item))
            .collect(),
        Some(item) => vec![(at, item)],
    }
}

/// The indexes `value` names: one integer, or an array of them.
fn indexes(value: &Value) -> Vec<u64> {
    match value {
        Value::Array(items) => items.iter().filter_map(Value::as_u64).collect(),
        other => other.as_u64().into_iter().collect(),
    }
}

/// What the subset's schema cannot state about a chart: that each series has the coordinate system
/// it is drawn in, that a series on axes has its two axes in one grid, that a `heatmap` has a
/// `visualMap` and two `category` axes, and that every index names an object the option has.
/// ECharts would otherwise fail to draw the chart, or draw it empty.
pub(super) fn check_chart(chart: &Chart, path: &str) -> Result<(), PageContentError> {
    let at = join(path, "option");
    let option = &chart.option;
    let fail =
        |path: String, message: String| Err(PageContentError::new(path, message, Some("chart")));
    let in_range = |value: Option<&Value>, path: String, what: &str, have: usize| {
        for wanted in value.map(indexes).unwrap_or_default() {
            if wanted as usize >= have {
                return fail(
                    path,
                    format!("names {what} {wanted}, but the option has {have}"),
                );
            }
        }
        Ok(())
    };
    let grids = count(option, "grid").max(1);
    let x_axes = count(option, "xAxis");
    let y_axes = count(option, "yAxis");
    // The axis under `key` a series names with `at` (the first when it names none), and its index.
    let axis = |key: &str, at: Option<&Value>| {
        let wanted = at.and_then(Value::as_u64).unwrap_or(0) as usize;
        let axis = match option.get(key)? {
            Value::Array(items) => items.get(wanted)?,
            axis => axis,
        };
        Some((wanted, axis))
    };
    let radars = count(option, "radar");
    let series_count = count(option, "series");
    for key in ["xAxis", "yAxis"] {
        for (path, axis) in each(option, key, &at) {
            in_range(
                axis.get("gridIndex"),
                join(&path, "gridIndex"),
                "grid",
                grids,
            )?;
        }
    }
    for (path, series) in each(option, "series", &at) {
        let kind = series
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if CARTESIAN.contains(&kind) {
            if x_axes == 0 || y_axes == 0 {
                return fail(
                    path,
                    format!(
                        "a `{kind}` series is drawn on axes: the option needs `xAxis` and `yAxis`"
                    ),
                );
            }
            in_range(
                series.get("xAxisIndex"),
                join(&path, "xAxisIndex"),
                "xAxis",
                x_axes,
            )?;
            in_range(
                series.get("yAxisIndex"),
                join(&path, "yAxisIndex"),
                "yAxis",
                y_axes,
            )?;
            let grid = |axis: &Value| axis.get("gridIndex").and_then(Value::as_u64).unwrap_or(0);
            let x = axis("xAxis", series.get("xAxisIndex"));
            let y = axis("yAxis", series.get("yAxisIndex"));
            if let (Some((x_at, x_axis)), Some((y_at, y_axis))) = (x, y) {
                if grid(x_axis) != grid(y_axis) {
                    return fail(
                        path,
                        format!(
                            "its xAxis {x_at} is in grid {} and its yAxis {y_at} in grid {}: a \
                             series is drawn on two axes of one grid",
                            grid(x_axis),
                            grid(y_axis)
                        ),
                    );
                }
            }
        }
        if kind == "radar" {
            if radars == 0 {
                return fail(
                    path,
                    "a `radar` series needs the option's `radar`".to_string(),
                );
            }
            in_range(
                series.get("radarIndex"),
                join(&path, "radarIndex"),
                "radar",
                radars,
            )?;
        }
        if kind == "heatmap" {
            if count(option, "visualMap") == 0 {
                return fail(
                    path,
                    "a `heatmap` series needs a `visualMap` to shade its values".to_string(),
                );
            }
            for (key, index_key) in [("xAxis", "xAxisIndex"), ("yAxis", "yAxisIndex")] {
                let Some((wanted, axis)) = axis(key, series.get(index_key)) else {
                    continue;
                };
                if axis.get("type").and_then(Value::as_str) != Some("category") {
                    return fail(
                        path,
                        format!(
                            "a `heatmap` series is drawn on two `category` axes, but its {key} \
                             {wanted} does not have `\"type\": \"category\"`"
                        ),
                    );
                }
            }
        }
    }
    for (path, zoom) in each(option, "dataZoom", &at) {
        in_range(
            zoom.get("xAxisIndex"),
            join(&path, "xAxisIndex"),
            "xAxis",
            x_axes,
        )?;
        in_range(
            zoom.get("yAxisIndex"),
            join(&path, "yAxisIndex"),
            "yAxis",
            y_axes,
        )?;
    }
    for (path, map) in each(option, "visualMap", &at) {
        in_range(
            map.get("seriesIndex"),
            join(&path, "seriesIndex"),
            "series",
            series_count,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::super::{Component, PageContent, PageContentError};

    fn page(chart: Value) -> Result<PageContent, PageContentError> {
        PageContent::parse(&json!({ "body": [chart] }))
    }

    fn chart(option: Value) -> Value {
        json!({ "type": "chart", "option": option })
    }

    fn bars() -> Value {
        json!({
            "xAxis": { "type": "category", "data": ["a", "b"] },
            "yAxis": { "type": "value" },
            "series": [{ "type": "bar", "name": "n", "data": [1, 2] }],
        })
    }

    /// The refusal of `option`: its path and message, with the `chart` kind's schema to read it by.
    fn refusal(option: Value) -> (String, String) {
        let error = page(chart(option)).expect_err("refused");
        assert_eq!(error.schema_name(), "chart", "{error}");
        (error.path, error.message)
    }

    /// A chart of every series type the subset has, with the components each needs, is accepted
    /// and reads back as it was written.
    #[test]
    fn every_series_type_is_accepted() {
        let option = json!({
            "title": { "text": "All", "subtext": "kinds", "left": "center", "textAlign": "center" },
            "legend": { "type": "scroll", "orient": "vertical", "right": 0, "top": "10%", "selectedMode": "single" },
            "tooltip": { "trigger": "axis", "axisPointer": { "type": "shadow" } },
            "grid": [{ "left": 40, "bottom": "15%" }, { "top": "60%" }, { "left": "60%" }],
            "xAxis": [
                { "type": "category", "data": ["a", "b", 3], "boundaryGap": false, "axisLabel": { "rotate": 45, "interval": 0 } },
                { "type": "value", "gridIndex": 1, "min": 0, "max": 10, "splitLine": { "show": false } },
                { "type": "category", "gridIndex": 2, "data": ["x", "y"] },
            ],
            "yAxis": [
                { "type": "log", "logBase": 10, "name": "Count" },
                { "type": "value", "gridIndex": 1, "scale": true },
                { "type": "category", "gridIndex": 2, "data": ["m", "n"] },
            ],
            "radar": { "indicator": [{ "name": "speed", "max": 10 }, { "name": "size" }], "shape": "circle" },
            "visualMap": { "type": "continuous", "min": 0, "max": 9, "calculable": true, "seriesIndex": 3, "text": ["high", "low"] },
            "dataZoom": [{ "type": "inside", "xAxisIndex": [0] }, { "type": "slider", "start": 10, "end": 90 }],
            "series": [
                { "type": "line", "name": "l", "data": [1, null, "-"], "smooth": 0.4, "step": "middle", "areaStyle": { "opacity": 0.3 }, "symbol": "diamond", "stack": "s" },
                { "type": "bar", "name": "b", "data": [{ "name": "x", "value": 2 }], "barWidth": "40%", "label": { "show": true, "position": "insideTop" }, "emphasis": { "focus": "series" } },
                { "type": "scatter", "data": [[1, 2], [3, 4]], "xAxisIndex": 1, "yAxisIndex": 1, "symbolSize": 8 },
                { "type": "heatmap", "data": [[0, 0, 5], [1, 0, 7]], "xAxisIndex": 2, "yAxisIndex": 2 },
                { "type": "pie", "radius": ["40%", "70%"], "center": ["50%", "50%"], "roseType": "area", "data": [{ "name": "p", "value": 3 }] },
                { "type": "radar", "data": [{ "name": "r", "value": [3, 4] }] },
                { "type": "funnel", "sort": "ascending", "gap": 2, "min": 0, "max": 100, "data": [{ "name": "f", "value": 60 }] },
                { "type": "gauge", "min": 0, "max": 100, "progress": { "show": true }, "data": [{ "value": 72, "name": "score" }] },
            ],
        });
        let tree = json!({
            "type": "chart",
            "option": option,
            "height": 480,
            "summary": "Every kind of series.",
        });
        let content = page(tree.clone()).expect("a valid chart");
        let Component::Chart { component } = &content.body[0] else {
            panic!("a chart");
        };
        assert_eq!(component.height, Some(480));
        assert_eq!(serde_json::to_value(&content.body[0]).unwrap(), tree);
    }

    /// What would make ECharts put a string into the window as HTML, load or open an address, or
    /// choose its own colours is outside the subset: refused at its path, with the reason.
    #[test]
    fn options_outside_the_subset_are_refused() {
        let with = |path: &[&str], value: Value| {
            let mut option = bars();
            let mut at = &mut option;
            for key in &path[..path.len() - 1] {
                at = match key.parse::<usize>() {
                    Ok(i) => &mut at[i],
                    Err(_) => {
                        if at.get(*key).is_none() {
                            at[*key] = json!({});
                        }
                        &mut at[*key]
                    }
                };
            }
            at[path[path.len() - 1]] = value;
            option
        };
        let cases = [
            (
                with(&["tooltip", "formatter"], json!("<b>{b}</b>")),
                "body[0].option.tooltip.formatter",
            ),
            (
                with(&["tooltip", "renderMode"], json!("html")),
                "body[0].option.tooltip.renderMode",
            ),
            (
                with(&["tooltip", "extraCssText"], json!("x")),
                "body[0].option.tooltip.extraCssText",
            ),
            (
                with(&["series", "0", "label", "formatter"], json!("{c}%")),
                "body[0].option.series[0].label.formatter",
            ),
            (
                with(&["series", "0", "label", "rich"], json!({})),
                "body[0].option.series[0].label.rich",
            ),
            (
                with(&["title", "link"], json!("https://example.com")),
                "body[0].option.title.link",
            ),
            (with(&["color"], json!(["#f00"])), "body[0].option.color"),
            (
                with(&["backgroundColor"], json!("#000")),
                "body[0].option.backgroundColor",
            ),
            (
                with(&["textStyle"], json!({ "fontFamily": "x" })),
                "body[0].option.textStyle",
            ),
            (
                with(&["series", "0", "itemStyle"], json!({ "color": "red" })),
                "body[0].option.series[0].itemStyle",
            ),
            (
                with(&["toolbox"], json!({ "feature": { "dataView": {} } })),
                "body[0].option.toolbox",
            ),
            (with(&["graphic"], json!([])), "body[0].option.graphic"),
            (
                with(&["dataset"], json!({ "source": [] })),
                "body[0].option.dataset",
            ),
            (
                with(&["xAxis", "axisLabel", "formatter"], json!("{value} ms")),
                "body[0].option.xAxis.axisLabel.formatter",
            ),
            (
                with(
                    &["series", "0", "data"],
                    json!([{ "value": 1, "itemStyle": {} }]),
                ),
                "body[0].option.series[0].data[0].itemStyle",
            ),
        ];
        for (option, path) in cases {
            let (at, message) = refusal(option);
            assert_eq!(at, path);
            assert!(message.starts_with("unknown property"), "{path}: {message}");
        }
        let (at, message) = refusal(with(
            &["series", "0", "symbol"],
            json!("image://https://example.com/x.png"),
        ));
        assert_eq!(at, "body[0].option.series[0].symbol");
        assert!(message.contains("`circle`"), "{message}");
        let (at, _) = refusal(with(&["series", "0", "type"], json!("map")));
        assert_eq!(at, "body[0].option.series[0].type");
        let (at, message) = refusal(json!({ "series": [{ "data": [1] }] }));
        assert_eq!(
            (at.as_str(), message.as_str()),
            (
                "body[0].option.series[0]",
                "missing required property `type`"
            )
        );
        for radar in [json!({ "shape": "circle" }), json!([{ "shape": "circle" }])] {
            let at_radar = if radar.is_array() {
                "body[0].option.radar[0]"
            } else {
                "body[0].option.radar"
            };
            let (at, message) =
                refusal(json!({ "radar": radar, "series": [{ "type": "radar", "data": [] }] }));
            assert_eq!(
                (at.as_str(), message.as_str()),
                (at_radar, "missing required property `indicator`")
            );
        }
        let (at, message) = refusal(json!({ "xAxis": {}, "yAxis": {} }));
        assert_eq!(
            (at.as_str(), message.as_str()),
            ("body[0].option", "missing required property `series`")
        );
    }

    /// The chart's own properties: `option` is required, `height` within its range, `summary` not
    /// blank.
    #[test]
    fn the_chart_properties_are_checked() {
        let error = page(json!({ "type": "chart" })).unwrap_err();
        assert_eq!(error.message, "missing required property `option`");
        let error = page(json!({ "type": "chart", "option": bars(), "height": 100 })).unwrap_err();
        assert_eq!(error.path, "body[0].height");
        let error = page(json!({ "type": "chart", "option": bars(), "height": 721 })).unwrap_err();
        assert_eq!(error.path, "body[0].height");
        let error = page(json!({ "type": "chart", "option": bars(), "summary": " " })).unwrap_err();
        assert_eq!(error.path, "body[0].summary");
        let error = page(json!({ "type": "chart", "option": [] })).unwrap_err();
        assert_eq!(error.path, "body[0].option");
    }

    /// A series without the coordinate system it is drawn in or with its axes in two grids, a
    /// `heatmap` without a `visualMap` or off `category` axes, and an index that names nothing are
    /// refused, at the property that is wrong.
    #[test]
    fn what_echarts_could_not_draw_is_refused() {
        let series = |series: Value| json!({ "series": [series] });
        let cases = [
            (
                series(json!({ "type": "line", "data": [1] })),
                "body[0].option.series[0]",
                "needs `xAxis` and `yAxis`",
            ),
            (
                series(json!({ "type": "radar", "data": [] })),
                "body[0].option.series[0]",
                "needs the option's `radar`",
            ),
            (
                json!({ "xAxis": {}, "yAxis": {}, "series": [{ "type": "heatmap", "data": [] }] }),
                "body[0].option.series[0]",
                "needs a `visualMap`",
            ),
            (
                json!({ "xAxis": { "type": "category" }, "yAxis": {}, "visualMap": {},
                        "series": [{ "type": "heatmap", "data": [] }] }),
                "body[0].option.series[0]",
                "its yAxis 0 does not have `\"type\": \"category\"`",
            ),
            (
                json!({ "xAxis": { "type": "value" }, "yAxis": { "type": "category" }, "visualMap": {},
                        "series": [{ "type": "heatmap", "data": [] }] }),
                "body[0].option.series[0]",
                "its xAxis 0 does not have",
            ),
            (
                json!({ "grid": [{}, {}], "xAxis": [{}, { "gridIndex": 1 }], "yAxis": {},
                        "series": [{ "type": "line", "data": [], "xAxisIndex": 1 }] }),
                "body[0].option.series[0]",
                "its xAxis 1 is in grid 1 and its yAxis 0 in grid 0",
            ),
            (
                json!({ "xAxis": {}, "yAxis": {}, "series": [{ "type": "bar", "data": [], "yAxisIndex": 1 }] }),
                "body[0].option.series[0].yAxisIndex",
                "names yAxis 1, but the option has 1",
            ),
            (
                json!({ "radar": { "indicator": [{ "name": "a" }] }, "series": [{ "type": "radar", "data": [], "radarIndex": 2 }] }),
                "body[0].option.series[0].radarIndex",
                "names radar 2",
            ),
            (
                json!({ "xAxis": { "gridIndex": 1 }, "yAxis": {}, "series": [{ "type": "bar", "data": [] }] }),
                "body[0].option.xAxis.gridIndex",
                "names grid 1, but the option has 1",
            ),
            (
                json!({ "xAxis": {}, "yAxis": {}, "dataZoom": [{ "type": "inside", "xAxisIndex": [0, 3] }], "series": [{ "type": "bar", "data": [] }] }),
                "body[0].option.dataZoom[0].xAxisIndex",
                "names xAxis 3",
            ),
            (
                json!({ "visualMap": { "seriesIndex": 1 }, "series": [{ "type": "pie", "data": [] }] }),
                "body[0].option.visualMap.seriesIndex",
                "names series 1",
            ),
        ];
        for (option, path, words) in cases {
            let (at, message) = refusal(option);
            assert_eq!(at, path);
            assert!(message.contains(words), "{path}: {message}");
        }
        page(chart(series(
            json!({ "type": "pie", "data": [{ "name": "a", "value": 1 }] }),
        )))
        .expect("a pie needs no axes");
    }

    /// A chart can be shown only for an answer, like any component; `visible_when` is checked the
    /// same way.
    #[test]
    fn a_chart_takes_visible_when() {
        let page = |when: Value| {
            PageContent::parse(&json!({ "body": [
                { "type": "toggle", "name": "more", "label": "More" },
                { "type": "chart", "option": bars(), "visible_when": when },
                { "type": "submit" },
            ] }))
        };
        page(json!({ "field": "more", "equals": true })).expect("a field of the page");
        let error = page(json!({ "field": "less", "equals": true })).unwrap_err();
        assert_eq!(error.path, "body[1].visible_when.field");
        assert_eq!(error.schema_name(), "chart");
    }
}
