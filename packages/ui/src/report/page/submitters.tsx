// The submitting components of a page. A press goes to the page's form, which checks the values and sends the
// submission; while it is out the pressed button is pending and the others are unavailable, and once the page is
// submitted or only a history page they stay as disabled buttons, the one that was pressed marked.
import React, { useId } from "react";

import { useFocusHandoff } from "../../components/focus/useFocusHandoff";
import { focusFirstIn } from "../../components/focus/regionFocus";
import { useT } from "../../i18n/react";
import { Button, Label } from "../../primitives";
import type { Component } from "../../protocol";
import { choiceKey } from "./formModel";
import { usePageForm } from "./PageFormContext";

type Of<K extends "submit" | "choices"> = Extract<Component, { type: K }>;

/** When the page locks after a submission, the submitter that held focus turns unavailable and focus would fall to
 * `<body>`; it goes to the start of the report panel instead. The caller spreads what this returns on a wrapper of its
 * submitters and renders on the form's changes, which it does as it reads the form. */
function useSubmitterHandoff(): ReturnType<typeof useFocusHandoff> {
  return useFocusHandoff(() => focusFirstIn("panel-report"));
}

export function SubmitView({ component }: { component: Of<"submit"> }): React.ReactElement {
  const t = useT();
  const form = usePageForm();
  const submitter = component.name !== undefined && component.value !== undefined ? { name: component.name, value: component.value } : undefined;
  const handoff = useSubmitterHandoff();
  const key = form.submitKeys.get(component) ?? "";
  const pending = form.pending && form.pressed === key;
  // After a submission the button that sent it is the one drawn as pressed; with none named, they all are.
  const sent = form.submission !== null && (submitter === undefined || form.submission[submitter.name] === submitter.value);
  return (
    <span className="contents" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      <Button
        variant={form.locked && !sent ? "secondary" : "primary"}
        pending={pending}
        disabled={form.locked && !pending}
        current={form.locked && sent && submitter !== undefined}
        onPress={() => form.press({ key, submitter })}
      >
        <span dir={component.label === undefined ? undefined : "auto"}>{component.label ?? t("report.submit")}</span>
      </Button>
    </span>
  );
}

export function ChoicesView({ component }: { component: Of<"choices"> }): React.ReactElement {
  const form = usePageForm();
  const labelId = useId();
  const handoff = useSubmitterHandoff();
  return (
    <div role="group" aria-labelledby={component.label ? labelId : undefined} className="flex flex-col gap-1.5" onFocus={handoff.onFocus} onBlur={handoff.onBlur}>
      {component.label && <Label id={labelId} dir="auto" className="align-match-parent">{component.label}</Label>}
      <div className="flex flex-wrap gap-2">
        {component.options.map((option) => {
          const key = choiceKey(component.name, option.value);
          const pending = form.pending && form.pressed === key;
          const sent = form.submission?.[component.name] === option.value;
          return (
            <Button
              key={option.value}
              variant={sent ? "primary" : "secondary"}
              pending={pending}
              disabled={form.locked && !pending}
              current={sent}
              onPress={() => form.press({ key, submitter: { name: component.name, value: option.value } })}
            >
              <span dir="auto">{option.label ?? option.value}</span>
            </Button>
          );
        })}
      </div>
    </div>
  );
}
