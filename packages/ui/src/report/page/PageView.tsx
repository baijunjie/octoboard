import React, { useEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";

import { StatusAnnouncer } from "../../components/announce/StatusAnnouncer";
import { useLatest } from "../../components/hooks/useLatest";
import { useT } from "../../i18n/react";
import type { Page, PageContent, SubmittedValue } from "../../protocol";
import { HeadingLevelContext } from "./headingLevel";
import { checkPattern } from "./patternCheck";
import { PageFormContext, type PageFormState, type Pressed } from "./PageFormContext";
import { PageTablesContext, tableNumbers } from "./PageTable";
import { Components } from "./registry";
import {
  issuesOf, pageVisibility, patternTargets, submissionOf, submitterKeys, valuesOfSubmission, walkComponents,
  type FieldValue, type FormValues, type PatternFinding,
} from "./formModel";

/** Where Enter, pressed in a single-line field, submits: the fields a form's implicit submission is for. */
const SUBMITTING_FIELDS = "input:is(:not([type]), [type=text], [type=date], [type=time], [type=datetime-local])";

/** What a failed submit puts focus on in a field: its control (a select's trigger is a button). */
const CONTROLS = "input:not([type=hidden]), textarea, button";

/**
 * One page's content as a form. The values the user edits are held by the owner, which passes them as `draft` (the
 * components' defaults until the user edits) and takes an update of them from `onDraft`, so that they outlive this view.
 * A page that is submitted (`submission`) shows the submitted values instead, read-only, which is the same in every window and
 * after a reload; so does a history page and a page whose submission is out (`pending`), which show what the user
 * has entered, read-only. A submitter's press checks the visible fields, and only when they are all valid hands the
 * submission to `onSubmit`, with the pressed submitter's `name: value` where it has one; otherwise the fields that
 * are wrong say so and focus goes to the first. Once a press has failed the fields are checked as the user edits them,
 * so an error goes as it is fixed, except a field's `pattern`, which is tested when the user leaves the field and when
 * the form is submitted, not on every keystroke, and in a Web Worker (`patternCheck.ts`): a press waits for the tests
 * that are out, with the form locked and the pressed button pending as for a submission that is out, and a value that
 * could not be tested in time counts as not matching. When the tests settle the page is read as it is then: a form that
 * was locked meanwhile (by another window's submission) or a view that is gone submits nothing.
 *
 * The page does not remember that a submission is out: `pending`, the `pressed` submitter (its `key`, which
 * `submitterKeys` gives a button and which names it the same after the page is listed anew) whose submission it is, and
 * `submission` come from the owner, which knows what became of the request.
 */
export function PageView({ content, submission, history, pending, pressed, draft, onDraft, onSubmit }: {
  content: PageContent;
  submission: Page["submission"];
  history: boolean;
  pending: boolean;
  pressed?: string;
  draft: FormValues;
  onDraft: (update: (held: FormValues) => FormValues) => void;
  onSubmit: (data: Record<string, SubmittedValue>, pressed: Pressed) => void;
}): React.ReactElement {
  const t = useT();
  const formRef = useRef<HTMLFormElement>(null);
  const [attempted, setAttempted] = useState(false);
  // What testing each text field's `pattern` came to, for the value it held when the user left it or the form was
  // submitted.
  const [findings, setFindings] = useState<ReadonlyMap<string, PatternFinding>>(new Map());
  // The submitter whose press is waiting for the tests of the patterns, which locks the form as a submission out does; the
  // ref keeps a second press in the same tick from starting another.
  const [checkingKey, setCheckingKey] = useState<string>();
  const checking = useRef(false);
  const alive = useRef(false);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const frozen = history || submission !== null || pending;
  const locked = frozen || checkingKey !== undefined;
  const values = useMemo(() => (submission ? valuesOfSubmission(content, submission) : draft), [content, submission, draft]);
  const tables = useMemo(() => tableNumbers(content.body), [content]);
  const isShown = useMemo(() => pageVisibility(content, values), [content, values]);
  const issues = useMemo(() => {
    if (frozen) return new Map();
    const found = issuesOf(content, values, isShown, findings);
    // Until a press has failed, only what leaving a field turned up is shown.
    return attempted ? found : new Map([...found].filter(([, issue]) => issue.kind === "pattern" || issue.kind === "patternUnchecked"));
  }, [attempted, frozen, content, values, isShown, findings]);
  const submitKeys = useMemo(() => submitterKeys(content), [content]);

  // What a press that has waited reads when its tests settle, not what the render that pressed held.
  const latest = useLatest({ content, values, isShown, frozen, findings, onSubmit });

  const settle = (pressed: Pressed, tested: ReadonlyMap<string, PatternFinding>) => {
    const now = latest.current;
    if (now.frozen) return;
    const tests = new Map([...now.findings, ...tested]);
    if (tested.size > 0) setFindings(tests);
    const wrong = issuesOf(now.content, now.values, now.isShown, tests);
    if (wrong.size > 0) {
      setAttempted(true);
      const first = [...wrong.keys()][0];
      const field = [...(formRef.current?.querySelectorAll<HTMLElement>("[data-page-field]") ?? [])].find((element) => element.dataset.pageField === first);
      field?.querySelector<HTMLElement>(CONTROLS)?.focus();
      return;
    }
    now.onSubmit(submissionOf(now.content, now.values, now.isShown, pressed.submitter), pressed);
  };

  const press: PageFormState["press"] = (pressed) => {
    if (locked || checking.current) return;
    const targets = patternTargets(content, values, isShown);
    if (targets.size === 0) {
      settle(pressed, new Map());
      return;
    }
    checking.current = true;
    setCheckingKey(pressed.key);
    void Promise.all([...targets].map(async ([name, { pattern, text }]): Promise<[string, PatternFinding]> => [name, { value: text, result: await checkPattern(pattern, text) }])).then((tested) => {
      checking.current = false;
      if (!alive.current) return;
      // Unlocked before the first wrong field is focused, so that a disabled control can take focus.
      flushSync(() => setCheckingKey(undefined));
      settle(pressed, new Map(tested));
    });
  };

  const form: PageFormState = {
    values,
    setValue: (name: string, value: FieldValue) => {
      onDraft((held) => ({ ...held, [name]: value }));
    },
    issues,
    locked,
    pending: pending || checkingKey !== undefined,
    pressed: pending ? pressed : checkingKey,
    checkPattern: (name) => {
      const target = patternTargets(content, values, isShown).get(name);
      if (target === undefined) return;
      void checkPattern(target.pattern, target.text).then((result) => setFindings((held) => new Map(held).set(name, { value: target.text, result })));
    },
    submitKeys,
    submission,
    isShown,
    press,
  };

  return (
    <PageFormContext.Provider value={form}>
      <StatusAnnouncer text={pending || checkingKey !== undefined ? t("report.submitting") : undefined} />
      <form
        ref={formRef}
        noValidate
        onSubmit={(event) => event.preventDefault()}
        onKeyDown={(event) => {
          if (event.key !== "Enter" || event.nativeEvent.isComposing || !(event.target instanceof HTMLElement) || !event.target.matches(SUBMITTING_FIELDS)) return;
          // As a browser's form does: Enter in a field presses the form's first submit button.
          const submit = [...walkComponents(content.body)].find((component) => component.type === "submit" && isShown(component));
          if (!submit || submit.type !== "submit") return;
          event.preventDefault();
          press({ key: submitKeys.get(submit) ?? "", submitter: submit.name !== undefined && submit.value !== undefined ? { name: submit.name, value: submit.value } : undefined });
        }}
        className="@container flex flex-col gap-6"
      >
        {content.title !== undefined && <h3 className="-mb-3.5 text-lg font-semibold align-match-parent" dir="auto">{content.title}</h3>}
        <HeadingLevelContext.Provider value={content.title !== undefined ? 3 : 2}>
          <PageTablesContext.Provider value={tables}>
            <Components list={content.body} />
          </PageTablesContext.Provider>
        </HeadingLevelContext.Provider>
      </form>
    </PageFormContext.Provider>
  );
}
