// The form a page's components make: the values its input fields hold, which components are shown, what is wrong with
// a value, and the submission the values come to. Pure, so every rule is testable without a rendered page; the
// components that draw the fields are in `fields.tsx`.
import type { Component, PageContent, SubmittedValue, VisibleEquals } from "../../protocol";
import type { PatternResult } from "./patternCheck";

/** The kinds of component that are an input field: each has a `name`, holds a value and is submitted with it. */
export type InputType = "text" | "textarea" | "number" | "date" | "time" | "datetime" | "radio" | "select" | "checkboxes" | "toggle" | "slider";

export type InputComponent = Extract<Component, { type: InputType }>;

/** What a field holds while the user edits it. A number is held as the text typed, so a half-typed value is not lost;
 * `submittedValue` makes the number of it. */
export type FieldValue = string | number | boolean | string[];

export type FormValues = Record<string, FieldValue>;

const INPUT_TYPES: ReadonlySet<string> = new Set<InputType>(["text", "textarea", "number", "date", "time", "datetime", "radio", "select", "checkboxes", "toggle", "slider"]);

export function isInput(component: Component): component is InputComponent {
  return INPUT_TYPES.has(component.type);
}

/** The components a container holds (a section's, a column layout's, every tab's, in order), none for any other kind. */
export function childrenOf(component: Component): readonly Component[] {
  switch (component.type) {
    case "section":
    case "columns":
      return component.children;
    case "tabs":
      return component.tabs.flatMap((tab) => tab.children);
    default:
      return [];
  }
}

/** The components of `list` and of the containers inside them, parents before the children they hold. */
export function* walkComponents(list: readonly Component[]): Generator<Component> {
  for (const component of list) {
    yield component;
    yield* walkComponents(childrenOf(component));
  }
}

/** The page's input fields in the order they are drawn. */
export function inputsOf(content: PageContent): InputComponent[] {
  return [...walkComponents(content.body)].filter(isInput);
}

/** What a field holds before the user has touched it: the component's own default, or empty. */
export function defaultValue(component: InputComponent): FieldValue {
  switch (component.type) {
    case "number":
      return component.default === undefined ? "" : String(component.default);
    case "slider":
      return component.default ?? component.min;
    case "toggle":
      return component.default ?? false;
    case "checkboxes":
      return component.default ?? [];
    default:
      return component.default ?? "";
  }
}

export function defaultValues(content: PageContent): FormValues {
  return Object.fromEntries(inputsOf(content).map((input) => [input.name, defaultValue(input)]));
}

/** The values a stored submission shows: what each field was submitted with, and for a field it left out (an empty
 * optional one, or one that was hidden) the empty value. */
export function valuesOfSubmission(content: PageContent, submission: Readonly<Record<string, SubmittedValue>>): FormValues {
  return Object.fromEntries(inputsOf(content).map((input) => {
    const submitted = submission[input.name];
    switch (input.type) {
      case "number":
        return [input.name, typeof submitted === "number" ? String(submitted) : ""];
      case "slider":
        return [input.name, typeof submitted === "number" ? submitted : input.min];
      case "toggle":
        return [input.name, submitted === true];
      case "checkboxes":
        return [input.name, Array.isArray(submitted) ? submitted : []];
      default:
        return [input.name, typeof submitted === "string" ? submitted : ""];
    }
  }));
}

/** A number as the user types it: `,` stands in for the decimal point when there is no `.`. `undefined` for an empty
 * text, `NaN` for one that is not a number. */
export function parseNumber(text: string): number | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return undefined;
  const normalised = !trimmed.includes(".") && trimmed.split(",").length === 2 ? trimmed.replace(",", ".") : trimmed;
  return /^[+-]?(\d+\.?\d*|\.\d+)(e[+-]?\d+)?$/i.test(normalised) ? Number(normalised) : Number.NaN;
}

function holds(input: InputComponent, value: FieldValue | undefined, equals: VisibleEquals): boolean {
  switch (input.type) {
    case "checkboxes":
      return Array.isArray(value) && typeof equals === "string" && value.includes(equals);
    case "toggle":
      return value === equals;
    case "number":
      return typeof equals === "number" && typeof value === "string" && parseNumber(value) === equals;
    case "slider":
      return typeof equals === "number" && value === equals;
    default:
      return typeof equals === "string" && value === equals;
  }
}

/** Whether a component is shown: every container around it is, and so is its own `visible_when`, which holds while the
 * field it names is shown and has the value it names. A field that is hidden holds no value, so what depends on it is
 * hidden too, and a condition that depends on itself through others is never met. */
export function pageVisibility(content: PageContent, values: FormValues): (component: Component) => boolean {
  const parents = new Map<Component, Component | undefined>();
  const fields = new Map<string, InputComponent>();
  const index = (list: readonly Component[], parent: Component | undefined) => {
    for (const component of list) {
      parents.set(component, parent);
      if (isInput(component)) fields.set(component.name, component);
      index(childrenOf(component), component);
    }
  };
  index(content.body, undefined);
  const known = new Map<Component, boolean>();
  const shown = (component: Component): boolean => {
    const cached = known.get(component);
    if (cached !== undefined) return cached;
    // Set while it is worked out, so a condition that leads back here ends as unmet.
    known.set(component, false);
    const parent = parents.get(component);
    let visible = parent === undefined || shown(parent);
    if (visible && component.visible_when) {
      const field = fields.get(component.visible_when.field);
      visible = field !== undefined && shown(field) && holds(field, values[field.name], component.visible_when.equals);
    }
    known.set(component, visible);
    return visible;
  };
  return shown;
}

/** What is wrong with a field's value, as data for the catalog to word (`issueText.ts`). `patternUnchecked` is a value
 * whose `pattern` could not be tested in time (`patternCheck.ts`), which counts as not matching. */
export type FieldIssue =
  | { kind: "required" | "requiredChoice" | "requiredSelection" | "pattern" | "patternUnchecked" | "number" }
  | { kind: "minLength" | "maxLength" | "minSelected" | "maxSelected"; count: number }
  | { kind: "min" | "max"; limit: number }
  | { kind: "notBefore" | "notAfter"; limit: string }
  | { kind: "step"; step: number };

/** What testing a field's `pattern` came to for the value the field held then. The test runs apart from the form
 * (`patternCheck.ts`), so a result is only good for the value it was found for. */
export interface PatternFinding {
  value: string;
  result: PatternResult;
}

/** The fields whose `pattern` is tested, by name: the visible single-line text fields that have one and hold a value. */
export function patternTargets(content: PageContent, values: FormValues, visible: (component: Component) => boolean): Map<string, { pattern: string; text: string }> {
  const targets = new Map<string, { pattern: string; text: string }>();
  for (const input of inputsOf(content)) {
    if (!visible(input) || input.type !== "text" || input.pattern === undefined) continue;
    const value = values[input.name];
    if (typeof value === "string" && value.trim() !== "") targets.set(input.name, { pattern: input.pattern, text: value });
  }
  return targets;
}

const characters = (text: string) => Array.from(text).length;

/** The first thing wrong with `value` as the value of `input`, if anything. An empty optional field is never wrong.
 * `found` is what testing the field's `pattern` came to, where it was tested. */
export function issueOf(input: InputComponent, value: FieldValue | undefined, found?: PatternResult): FieldIssue | undefined {
  switch (input.type) {
    case "text":
    case "textarea": {
      const text = typeof value === "string" ? value : "";
      if (text.trim() === "") return input.required ? { kind: "required" } : undefined;
      if (found === "mismatch") return { kind: "pattern" };
      if (found === "unchecked") return { kind: "patternUnchecked" };
      if (input.min_length !== undefined && characters(text) < input.min_length) return { kind: "minLength", count: input.min_length };
      if (input.max_length !== undefined && characters(text) > input.max_length) return { kind: "maxLength", count: input.max_length };
      return undefined;
    }
    case "number": {
      const parsed = parseNumber(typeof value === "string" ? value : "");
      if (parsed === undefined) return input.required ? { kind: "required" } : undefined;
      if (Number.isNaN(parsed)) return { kind: "number" };
      if (input.min !== undefined && parsed < input.min) return { kind: "min", limit: input.min };
      if (input.max !== undefined && parsed > input.max) return { kind: "max", limit: input.max };
      if (input.step !== undefined) {
        const steps = (parsed - (input.min ?? 0)) / input.step;
        if (Math.abs(steps - Math.round(steps)) > 1e-9 * Math.max(1, Math.abs(steps))) return { kind: "step", step: input.step };
      }
      return undefined;
    }
    case "date":
    case "time":
    case "datetime": {
      // The control's value is `YYYY-MM-DD`, `HH:MM` or `YYYY-MM-DDTHH:MM`, which order as text as they do in time.
      const text = typeof value === "string" ? value : "";
      if (text === "") return input.required ? { kind: "required" } : undefined;
      if (input.min !== undefined && text < input.min) return { kind: "notBefore", limit: input.min };
      if (input.max !== undefined && text > input.max) return { kind: "notAfter", limit: input.max };
      return undefined;
    }
    case "radio":
    case "select":
      return input.required && (typeof value !== "string" || value === "") ? { kind: "requiredChoice" } : undefined;
    case "checkboxes": {
      const chosen = Array.isArray(value) ? value.length : 0;
      if (chosen === 0 && input.required) return { kind: "requiredSelection" };
      if (input.min_selected !== undefined && chosen > 0 && chosen < input.min_selected) return { kind: "minSelected", count: input.min_selected };
      if (input.max_selected !== undefined && chosen > input.max_selected) return { kind: "maxSelected", count: input.max_selected };
      return undefined;
    }
    case "toggle":
    case "slider":
      return undefined;
  }
}

/** What is wrong with each visible field, by name, in the order the fields are drawn; `findings` hold what testing the
 * fields' `pattern` came to, each for the value it was found for. */
export function issuesOf(content: PageContent, values: FormValues, visible: (component: Component) => boolean, findings: ReadonlyMap<string, PatternFinding> = new Map()): Map<string, FieldIssue> {
  const issues = new Map<string, FieldIssue>();
  for (const input of inputsOf(content)) {
    if (!visible(input)) continue;
    const value = values[input.name];
    const finding = findings.get(input.name);
    const issue = issueOf(input, value, finding !== undefined && finding.value === value ? finding.result : undefined);
    if (issue) issues.set(input.name, issue);
  }
  return issues;
}

/** What a field is submitted with, or `undefined` for an empty optional field (a text of only blanks is empty), which is
 * left out of the submission: the message into the session then has no line for a field the user did not fill in. A
 * multiple choice is in the order of its options, whatever order it was ticked in. */
export function submittedValue(input: InputComponent, value: FieldValue | undefined): SubmittedValue | undefined {
  switch (input.type) {
    case "number": {
      const parsed = parseNumber(typeof value === "string" ? value : "");
      return parsed === undefined || Number.isNaN(parsed) ? undefined : parsed;
    }
    case "slider":
      return typeof value === "number" ? value : input.min;
    case "toggle":
      return value === true;
    case "checkboxes": {
      const chosen = Array.isArray(value) ? value : [];
      const ordered = input.options.map((option) => option.value).filter((option) => chosen.includes(option));
      return ordered.length > 0 ? ordered : undefined;
    }
    default:
      return typeof value === "string" && value.trim() !== "" ? value : undefined;
  }
}

/** The submission: each visible field's value and, for a pressed submitter that has a name, `name: value`. A hidden
 * field is not in it. */
export function submissionOf(
  content: PageContent,
  values: FormValues,
  visible: (component: Component) => boolean,
  submitter?: { name: string; value: string },
): Record<string, SubmittedValue> {
  const submission: Record<string, SubmittedValue> = {};
  for (const input of inputsOf(content)) {
    if (!visible(input)) continue;
    const value = submittedValue(input, values[input.name]);
    if (value !== undefined) submission[input.name] = value;
  }
  if (submitter) submission[submitter.name] = submitter.value;
  return submission;
}

/** The key of each `submit` button, by its place among the page's `submit` buttons (counted from 0 in the order they are
 * drawn, shown or not). A button with no name has nothing but its place to be told from another by, which also holds
 * when the page is listed anew and its components are new objects. A `choices` button is keyed by its `name: value`
 * (`choiceKey`). */
export function submitterKeys(content: PageContent): Map<Component, string> {
  const keys = new Map<Component, string>();
  for (const component of walkComponents(content.body)) {
    if (component.type === "submit") keys.set(component, `submit:${keys.size}`);
  }
  return keys;
}

/** The key of one button of a `choices` component. */
export function choiceKey(name: string, value: string): string {
  return `choice:${JSON.stringify([name, value])}`;
}
