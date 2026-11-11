// The input components of a page, each drawn with the application's own form controls. They read the page's form
// (`PageFormContext.tsx`) for the value, what is wrong with it and whether it may be edited, and write the value back; a
// field that may not be edited is read-only where its control has that state, and disabled where it does not (a
// select, a slider), and drawn as a value to read (`locked`).
import React, { useId } from "react";

import { FieldErrorMessage } from "../../components/form/FieldErrorMessage";
import { LabelledGroup } from "../../components/form/LabelledGroup";
import { OptionSelect } from "../../components/form/OptionSelect";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { Checkbox, Description, Input, Label, Radio, RadioGroup, Slider, Switch, TextArea, TextField } from "../../primitives";
import type { Component } from "../../protocol";
import { usePageForm } from "./PageFormContext";
import { issueText } from "./issueText";
import type { InputComponent } from "./formModel";

type Of<K extends InputComponent["type"]> = Extract<Component, { type: K }>;

/** The element a field sits in, which is what a failed submit finds the field by to move focus to it. */
function Field({ component, children }: { component: InputComponent; children: React.ReactNode }): React.ReactElement {
  return <div data-page-field={component.name}>{children}</div>;
}

const ALIGN = "align-match-parent";

/** The message under a field, for the field to hold: its error while it is invalid. */
function useIssueText(component: InputComponent): string | undefined {
  const t = useT();
  const language = useCurrentLanguage();
  const issue = usePageForm().issues.get(component.name);
  return issue && issueText(t, language, issue);
}

function textValue(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** A single-line field: text, a number typed as text, or a date, a time or a date and time the system's own control
 * picks. */
function LineField({ component, type, inputMode }: {
  component: Of<"text" | "number" | "date" | "time" | "datetime">;
  type?: "date" | "time" | "datetime-local";
  inputMode?: "decimal";
}): React.ReactElement {
  const form = usePageForm();
  const error = useIssueText(component);
  const placeholder = "placeholder" in component ? component.placeholder : undefined;
  return (
    <Field component={component}>
      <TextField
        fullWidth
        value={textValue(form.values[component.name])}
        onChange={(value) => form.setValue(component.name, value)}
        readOnly={form.locked}
        locked={form.locked}
        invalid={error !== undefined}
        type={type}
        inputMode={inputMode}
        onBlur={() => form.checkPattern(component.name)}
      >
        <Label dir="auto" className={ALIGN}>{component.label}</Label>
        <Input data-1p-ignore required={component.required} dir={type === undefined && component.type === "text" ? "auto" : undefined} placeholder={placeholder} />
        {component.description && <Description dir="auto" className={ALIGN}>{component.description}</Description>}
        <FieldErrorMessage>{error}</FieldErrorMessage>
      </TextField>
    </Field>
  );
}

export function TextFieldView({ component }: { component: Of<"text"> }): React.ReactElement {
  return <LineField component={component} />;
}

export function NumberFieldView({ component }: { component: Of<"number"> }): React.ReactElement {
  return <LineField component={component} inputMode="decimal" />;
}

export function DateFieldView({ component }: { component: Of<"date"> }): React.ReactElement {
  return <LineField component={component} type="date" />;
}

export function TimeFieldView({ component }: { component: Of<"time"> }): React.ReactElement {
  return <LineField component={component} type="time" />;
}

export function DatetimeFieldView({ component }: { component: Of<"datetime"> }): React.ReactElement {
  return <LineField component={component} type="datetime-local" />;
}

export function TextareaFieldView({ component }: { component: Of<"textarea"> }): React.ReactElement {
  const form = usePageForm();
  const error = useIssueText(component);
  return (
    <Field component={component}>
      <TextField
        fullWidth
        value={textValue(form.values[component.name])}
        onChange={(value) => form.setValue(component.name, value)}
        readOnly={form.locked}
        locked={form.locked}
        invalid={error !== undefined}
        onBlur={() => form.checkPattern(component.name)}
      >
        <Label dir="auto" className={ALIGN}>{component.label}</Label>
        <TextArea rows={component.rows ?? 4} required={component.required} dir="auto" placeholder={component.placeholder} />
        {component.description && <Description dir="auto" className={ALIGN}>{component.description}</Description>}
        <FieldErrorMessage>{error}</FieldErrorMessage>
      </TextField>
    </Field>
  );
}

export function RadioFieldView({ component }: { component: Of<"radio"> }): React.ReactElement {
  const form = usePageForm();
  const error = useIssueText(component);
  return (
    <Field component={component}>
      <RadioGroup
        label={component.label}
        labelDir="auto"
        labelClassName={ALIGN}
        value={textValue(form.values[component.name])}
        onChange={(value) => form.setValue(component.name, value)}
        readOnly={form.locked}
        invalid={error !== undefined}
        required={component.required}
      >
        {component.options.map((option) => (
          <Radio key={option.value} value={option.value} dir="auto">{option.label ?? option.value}</Radio>
        ))}
        {component.description && <Description dir="auto" className={ALIGN}>{component.description}</Description>}
        <FieldErrorMessage>{error}</FieldErrorMessage>
      </RadioGroup>
    </Field>
  );
}

export function SelectFieldView({ component }: { component: Of<"select"> }): React.ReactElement {
  const t = useT();
  const form = usePageForm();
  const error = useIssueText(component);
  const value = textValue(form.values[component.name]);
  return (
    <Field component={component}>
      <OptionSelect
        label={component.label}
        options={component.options.map((option) => ({ value: option.value, label: option.label ?? option.value }))}
        value={value === "" ? null : value}
        onChange={(next: string | null) => form.setValue(component.name, next ?? "")}
        variant="primary"
        placeholder={component.placeholder ?? t("report.selectPlaceholder")}
        description={component.description}
        errorMessage={error}
        isDisabled={form.locked}
        locked={form.locked}
        textDir="auto"
      />
    </Field>
  );
}

export function CheckboxesFieldView({ component }: { component: Of<"checkboxes"> }): React.ReactElement {
  const form = usePageForm();
  const error = useIssueText(component);
  const errorId = useId();
  const held = form.values[component.name];
  const chosen = Array.isArray(held) ? held : [];
  return (
    <Field component={component}>
      <LabelledGroup label={component.label} description={component.description} dir="auto" errorMessage={error} errorId={errorId}>
        {component.options.map((option) => (
          <Checkbox
            key={option.value}
            selected={chosen.includes(option.value)}
            dir="auto"
            labelClassName={ALIGN}
            readOnly={form.locked}
            describedBy={error ? errorId : undefined}
            onSelectedChange={(selected) => form.setValue(component.name, selected ? [...chosen, option.value] : chosen.filter((value) => value !== option.value))}
          >
            {option.label ?? option.value}
          </Checkbox>
        ))}
      </LabelledGroup>
    </Field>
  );
}

export function ToggleFieldView({ component }: { component: Of<"toggle"> }): React.ReactElement {
  const form = usePageForm();
  const descriptionId = useId();
  return (
    <Field component={component}>
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-0.5">
          <Label dir="auto" className={ALIGN}>{component.label}</Label>
          {component.description && <Description id={descriptionId} dir="auto" className={ALIGN}>{component.description}</Description>}
        </div>
        <Switch
          label={component.label}
          describedBy={component.description ? descriptionId : undefined}
          selected={form.values[component.name] === true}
          onSelectedChange={(selected) => form.setValue(component.name, selected)}
          unavailable={form.locked}
        />
      </div>
    </Field>
  );
}

export function SliderFieldView({ component }: { component: Of<"slider"> }): React.ReactElement {
  const form = usePageForm();
  const descriptionId = useId();
  const held = form.values[component.name];
  return (
    <Field component={component}>
      <div className="flex flex-col gap-1">
        <Slider
          label={component.label}
          labelDir="auto"
          value={typeof held === "number" ? held : component.min}
          onChange={(value) => form.setValue(component.name, value)}
          min={component.min}
          max={component.max}
          step={component.step}
          describedBy={component.description ? descriptionId : undefined}
          disabled={form.locked}
          locked={form.locked}
        />
        {component.description && <Description id={descriptionId} dir="auto" className={ALIGN}>{component.description}</Description>}
      </div>
    </Field>
  );
}
