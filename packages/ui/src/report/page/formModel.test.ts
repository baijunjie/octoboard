import { describe, expect, it } from "vitest";

import type { Component, PageContent } from "../../protocol";
import { childrenOf, defaultValues, inputsOf, issueOf, issuesOf, pageVisibility, parseNumber, patternTargets, submissionOf, valuesOfSubmission, walkComponents, type InputComponent, type PatternFinding } from "./formModel";

const input = <T extends InputComponent["type"]>(type: T, extra: Record<string, unknown>) =>
  ({ type, name: "f", label: "F", ...extra }) as unknown as Extract<InputComponent, { type: T }>;

describe("parseNumber", () => {
  it.each([["", undefined], ["  ", undefined], ["12", 12], ["-1.5", -1.5], ["1,5", 1.5], [".5", 0.5], ["1e3", 1000]])("reads %j as %j", (text, expected) => {
    expect(parseNumber(text)).toBe(expected);
  });
  it.each(["abc", "1,2,3", "1.2.3", "--1", "1 2", "0x10"])("rejects %j", (text) => {
    expect(parseNumber(text)).toBeNaN();
  });
});

describe("submittedValue", () => {
  it("leaves out an optional text of only blanks, as an empty one", () => {
    const content: PageContent = { body: [{ type: "text", name: "a", label: "A" }, { type: "textarea", name: "b", label: "B" }, { type: "text", name: "c", label: "C" }] };
    const values = { a: "  \n ", b: "\t", c: " kept " };
    expect(submissionOf(content, values, pageVisibility(content, values))).toEqual({ c: " kept " });
  });
});

describe("issueOf", () => {
  it("lets an empty optional field through and refuses an empty required one, blank text included", () => {
    expect(issueOf(input("text", {}), "")).toBeUndefined();
    expect(issueOf(input("text", { required: true }), "  ")).toEqual({ kind: "required" });
    expect(issueOf(input("number", { required: true }), "")).toEqual({ kind: "required" });
    expect(issueOf(input("date", { required: true }), "")).toEqual({ kind: "required" });
    expect(issueOf(input("select", { required: true, options: [{ value: "a" }] }), "")).toEqual({ kind: "requiredChoice" });
    expect(issueOf(input("radio", { required: true, options: [{ value: "a" }] }), "")).toEqual({ kind: "requiredChoice" });
    expect(issueOf(input("checkboxes", { required: true, options: [{ value: "a" }] }), [])).toEqual({ kind: "requiredSelection" });
  });

  it("says a value does not match its pattern, or could not be tested against it, only where it was tested", () => {
    const field = input("text", { pattern: "[a-z]+", min_length: 5 });
    expect(issueOf(field, "ABC")).toEqual({ kind: "minLength", count: 5 });
    expect(issueOf(field, "ABC", "match")).toEqual({ kind: "minLength", count: 5 });
    expect(issueOf(field, "ABC", "mismatch")).toEqual({ kind: "pattern" });
    expect(issueOf(field, "ABC", "unchecked")).toEqual({ kind: "patternUnchecked" });
    expect(issueOf(field, "  ", "mismatch")).toBeUndefined();
  });

  it("uses a finding only for the value it was found for", () => {
    const content: PageContent = { body: [{ type: "text", name: "a", label: "A", pattern: "x" }, { type: "text", name: "b", label: "B", pattern: "x" }] };
    const values = { a: "no", b: "no" };
    const visible = pageVisibility(content, values);
    expect([...issuesOf(content, values, visible).keys()]).toEqual([]);
    const findings = new Map<string, PatternFinding>([["a", { value: "no", result: "mismatch" }], ["b", { value: "n", result: "mismatch" }]]);
    expect([...issuesOf(content, values, visible, findings).keys()]).toEqual(["a"]);
  });

  it("names the visible single-line fields with a pattern and a value as the ones to test", () => {
    const content: PageContent = {
      body: [
        { type: "text", name: "a", label: "A", pattern: "x" },
        { type: "text", name: "blank", label: "B", pattern: "x" },
        { type: "text", name: "plain", label: "P" },
        { type: "text", name: "hidden", label: "H", pattern: "x", visible_when: { field: "a", equals: "other" } },
      ],
    };
    const values = { a: "no", blank: " ", plain: "v", hidden: "v" };
    expect(patternTargets(content, values, pageVisibility(content, values))).toEqual(new Map([["a", { pattern: "x", text: "no" }]]));
  });

  it("counts a length in characters", () => {
    expect(issueOf(input("text", { min_length: 2 }), "a")).toEqual({ kind: "minLength", count: 2 });
    expect(issueOf(input("text", { max_length: 2 }), "abc")).toEqual({ kind: "maxLength", count: 2 });
    expect(issueOf(input("textarea", { max_length: 1 }), "😀")).toBeUndefined();
  });

  it("checks a number's type, range and step from its minimum", () => {
    expect(issueOf(input("number", {}), "x")).toEqual({ kind: "number" });
    expect(issueOf(input("number", { min: 1 }), "0")).toEqual({ kind: "min", limit: 1 });
    expect(issueOf(input("number", { max: 1 }), "2")).toEqual({ kind: "max", limit: 1 });
    expect(issueOf(input("number", { min: 1, step: 0.5 }), "1.5")).toBeUndefined();
    expect(issueOf(input("number", { min: 1, step: 0.5 }), "1.25")).toEqual({ kind: "step", step: 0.5 });
    expect(issueOf(input("number", { step: 0.1 }), "0.3")).toBeUndefined();
  });

  it("orders dates and times as text", () => {
    expect(issueOf(input("date", { min: "2026-01-02" }), "2026-01-01")).toEqual({ kind: "notBefore", limit: "2026-01-02" });
    expect(issueOf(input("time", { max: "10:00" }), "10:01")).toEqual({ kind: "notAfter", limit: "10:00" });
    expect(issueOf(input("datetime", { min: "2026-01-01T10:00", max: "2026-01-01T12:00" }), "2026-01-01T11:00")).toBeUndefined();
  });

  it("bounds how many options are chosen, without calling an untouched optional group wrong", () => {
    const field = input("checkboxes", { options: [{ value: "a" }, { value: "b" }, { value: "c" }], min_selected: 2, max_selected: 2 });
    expect(issueOf(field, [])).toBeUndefined();
    expect(issueOf(field, ["a"])).toEqual({ kind: "minSelected", count: 2 });
    expect(issueOf(field, ["a", "b", "c"])).toEqual({ kind: "maxSelected", count: 2 });
    expect(issueOf(field, ["a", "b"])).toBeUndefined();
  });
});

const content: PageContent = {
  body: [
    { type: "radio", name: "kind", label: "Kind", options: [{ value: "bug" }, { value: "idea" }] },
    { type: "text", name: "steps", label: "Steps", visible_when: { field: "kind", equals: "bug" } },
    {
      type: "section", heading: "More", visible_when: { field: "kind", equals: "idea" },
      children: [{ type: "text", name: "why", label: "Why" }],
    },
    { type: "checkboxes", name: "tags", label: "Tags", options: [{ value: "a" }, { value: "b" }] },
    { type: "toggle", name: "urgent", label: "Urgent", default: false },
    { type: "text", name: "reason", label: "Reason", visible_when: { field: "urgent", equals: true } },
    { type: "text", name: "extra", label: "Extra", visible_when: { field: "tags", equals: "b" } },
    { type: "number", name: "n", label: "N" },
    { type: "text", name: "big", label: "Big", visible_when: { field: "n", equals: 5 } },
    { type: "text", name: "chained", label: "Chained", visible_when: { field: "steps", equals: "x" } },
    { type: "text", name: "a", label: "A", visible_when: { field: "b", equals: "1" } },
    { type: "text", name: "b", label: "B", visible_when: { field: "a", equals: "1" }, default: "1" },
  ],
};
const named = (name: string): Component => {
  const find = (list: readonly Component[]): Component | undefined => {
    for (const component of list) {
      if ("name" in component && component.name === name) return component;
      if (component.type === "section") {
        const inside = find(component.children);
        if (inside) return inside;
      }
    }
    return undefined;
  };
  return find(content.body)!;
};

describe("pageVisibility", () => {
  const shown = (values: Record<string, string | number | boolean | string[]>, name: string) =>
    pageVisibility(content, { ...defaultValues(content), ...values })(named(name));

  it("shows a component while the field it names holds the value, a selection holding it, a number equal to it", () => {
    expect(shown({}, "steps")).toBe(false);
    expect(shown({ kind: "bug" }, "steps")).toBe(true);
    expect(shown({ kind: "idea" }, "why")).toBe(true);
    expect(shown({ kind: "bug" }, "why")).toBe(false);
    expect(shown({ urgent: true }, "reason")).toBe(true);
    expect(shown({ tags: ["a"] }, "extra")).toBe(false);
    expect(shown({ tags: ["b", "a"] }, "extra")).toBe(true);
    expect(shown({ n: "5" }, "big")).toBe(true);
    expect(shown({ n: "5,0" }, "big")).toBe(true);
    expect(shown({ n: "6" }, "big")).toBe(false);
  });

  it("hides what depends on a field that is hidden, and never meets a condition that depends on itself", () => {
    expect(shown({ kind: "idea", steps: "x" }, "chained")).toBe(false);
    expect(shown({ kind: "bug", steps: "x" }, "chained")).toBe(true);
    expect(shown({ a: "1" }, "a")).toBe(false);
    expect(shown({ a: "1", b: "1" }, "b")).toBe(false);
  });

  it("leaves hidden fields out of the validation and the submission", () => {
    const strict: PageContent = { body: [
      { type: "toggle", name: "on", label: "On" },
      { type: "text", name: "needed", label: "Needed", required: true, visible_when: { field: "on", equals: true } },
    ] };
    const values = { on: false, needed: "" };
    const visible = pageVisibility(strict, values);
    expect(issuesOf(strict, values, visible).size).toBe(0);
    expect(submissionOf(strict, { on: false, needed: "typed" }, visible)).toEqual({ on: false });
    const shownNow = pageVisibility(strict, { on: true, needed: "" });
    expect([...issuesOf(strict, { on: true, needed: "" }, shownNow).keys()]).toEqual(["needed"]);
  });
});

describe("submissionOf", () => {
  const form: PageContent = { body: [
    { type: "text", name: "t", label: "T" },
    { type: "text", name: "empty", label: "Empty" },
    { type: "number", name: "n", label: "N" },
    { type: "number", name: "blank", label: "Blank" },
    { type: "slider", name: "s", label: "S", min: 0, max: 10, default: 4 },
    { type: "toggle", name: "on", label: "On" },
    { type: "select", name: "pick", label: "Pick", options: [{ value: "a" }, { value: "b" }] },
    { type: "checkboxes", name: "many", label: "Many", options: [{ value: "a" }, { value: "b" }, { value: "c" }], default: ["c", "a"] },
    { type: "checkboxes", name: "none", label: "None", options: [{ value: "a" }] },
  ] };

  it("sends strings, numbers, a boolean and a selection in option order, and leaves empty optional fields out", () => {
    const values = { ...defaultValues(form), t: "hello", n: "2,5", pick: "b" };
    expect(submissionOf(form, values, () => true)).toEqual({ t: "hello", n: 2.5, s: 4, on: false, pick: "b", many: ["a", "c"] });
  });

  it("adds the pressed submitter's name and value", () => {
    expect(submissionOf({ body: [] }, {}, () => true, { name: "action", value: "approve" })).toEqual({ action: "approve" });
  });

  it("reads a stored submission back as the values the fields show", () => {
    const submitted = { t: "hello", n: 2.5, s: 7, on: true, pick: "b", many: ["a", "c"] };
    expect(valuesOfSubmission(form, submitted)).toEqual({ t: "hello", empty: "", n: "2.5", blank: "", s: 7, on: true, pick: "b", many: ["a", "c"], none: [] });
  });
});

describe("containers", () => {
  const form: PageContent = { body: [
    { type: "toggle", name: "more", label: "More" },
    { type: "columns", children: [
      { type: "text", name: "left", label: "Left", default: "l" },
      { type: "section", heading: "Right", children: [{ type: "number", name: "right", label: "Right", required: true }] },
    ] },
    { type: "tabs", tabs: [
      { label: "One", children: [{ type: "text", name: "one", label: "One", default: "1" }] },
      { label: "Two", children: [
        { type: "text", name: "two", label: "Two", required: true },
        { type: "columns", children: [{ type: "checkboxes", name: "deep", label: "Deep", options: [{ value: "a" }], default: ["a"] }] },
      ] },
    ] },
    { type: "columns", visible_when: { field: "more", equals: true }, children: [{ type: "text", name: "hiddenColumn", label: "Hidden column", default: "h", required: true }] },
    { type: "tabs", visible_when: { field: "more", equals: true }, tabs: [
      { label: "A", children: [{ type: "text", name: "hiddenTab", label: "Hidden tab", default: "t" }] },
      { label: "B", children: [{ type: "columns", children: [{ type: "text", name: "hiddenDeep", label: "Hidden deep", default: "d" }] }] },
    ] },
  ] };
  const find = (name: string) => inputsOf(form).find((input) => input.name === name)!;

  it("lists the components a container holds: a section's and a column layout's children, every tab's children in order, none for a leaf", () => {
    const [, columns, tabs] = form.body;
    expect(childrenOf(columns)).toBe((columns as Extract<typeof columns, { type: "columns" }>).children);
    expect(childrenOf(tabs).map((child) => child.type)).toEqual(["text", "text", "columns"]);
    expect(childrenOf(form.body[0])).toEqual([]);
  });

  it("walks the components of columns and of every tab, parents before the components they hold", () => {
    const names = [...walkComponents(form.body)].map((component) => ("name" in component ? component.name : component.type));
    expect(names.slice(0, 11)).toEqual(["more", "columns", "left", "section", "right", "tabs", "one", "two", "columns", "deep", "columns"]);
    expect(inputsOf(form).map((input) => input.name)).toEqual(["more", "left", "right", "one", "two", "deep", "hiddenColumn", "hiddenTab", "hiddenDeep"]);
  });

  it("gives the fields inside columns and tabs their default values", () => {
    expect(defaultValues(form)).toMatchObject({ left: "l", right: "", one: "1", two: "", deep: ["a"] });
  });

  it("validates a field inside columns, in a section in them, and in a tab, shown or not", () => {
    const values = defaultValues(form);
    const issues = issuesOf(form, values, pageVisibility(form, values));
    expect([...issues.keys()].sort()).toEqual(["right", "two"]);
  });

  it("submits the fields inside columns and tabs, those of every tab", () => {
    const values = { ...defaultValues(form), right: "3", two: "second" };
    expect(submissionOf(form, values, pageVisibility(form, values))).toEqual({ more: false, left: "l", right: 3, one: "1", two: "second", deep: ["a"] });
  });

  it("hides what is inside a container whose own visible_when is unmet, at any depth, and shows it when it is met", () => {
    const hidden = pageVisibility(form, defaultValues(form));
    for (const name of ["hiddenColumn", "hiddenTab", "hiddenDeep"]) expect(hidden(find(name))).toBe(false);
    expect(hidden(form.body[3])).toBe(false);
    expect(hidden(find("left"))).toBe(true);
    expect(hidden(find("deep"))).toBe(true);

    const values = { ...defaultValues(form), more: true };
    const shown = pageVisibility(form, values);
    for (const name of ["hiddenColumn", "hiddenTab", "hiddenDeep"]) expect(shown(find(name))).toBe(true);
  });

  it("leaves the fields of a hidden container out of the validation and the submission", () => {
    const values = { ...defaultValues(form), right: "3", two: "x", hiddenColumn: "" };
    const visible = pageVisibility(form, values);
    expect(issuesOf(form, values, visible).has("hiddenColumn")).toBe(false);
    expect(submissionOf(form, values, visible)).not.toHaveProperty("hiddenColumn");
    expect(submissionOf(form, values, visible)).not.toHaveProperty("hiddenTab");
    expect(submissionOf(form, values, visible)).not.toHaveProperty("hiddenDeep");

    const open = { ...values, more: true };
    expect(issuesOf(form, open, pageVisibility(form, open)).has("hiddenColumn")).toBe(true);
    expect(submissionOf(form, { ...open, hiddenColumn: "typed" }, pageVisibility(form, open))).toMatchObject({ hiddenColumn: "typed", hiddenTab: "t", hiddenDeep: "d" });
  });

  it("reads a stored submission back into the fields inside columns and tabs", () => {
    expect(valuesOfSubmission(form, { left: "L", one: "uno", deep: [] })).toMatchObject({ left: "L", one: "uno", deep: [] });
  });
});
