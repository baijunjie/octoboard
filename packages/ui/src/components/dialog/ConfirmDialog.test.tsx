// @vitest-environment jsdom
import { act, StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers, runOutTimers } from "../../test/clock";
import { errorAlerts, type AcknowledgementView } from "../alerts/errorAlerts";
import { currentLanguage, setLanguage } from "../../i18n/language";
import { DaemonProvider } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";
import { failureOf, framesAsTimers, openable, settleFrames, trackMountRefocus, withModality } from "./dialogTestSupport";
import { Checkbox } from "../../primitives";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Renders a confirmation and takes it down when the test finishes. */
function mount(props: { errorTitle?: string; onConfirm?: () => Promise<void>; destructive?: boolean; typeToConfirm?: string; hideCancel?: boolean; confirmDisabled?: boolean; subject?: React.ReactNode; message?: string; unprompted?: boolean; failure?: AcknowledgementView; onCancel?: () => void }): void {
  // A previous test's script click leaves virtual modality, which defers react-aria's opening focus to a frame.
  withModality("pointer");
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <ConfirmDialog title="Archive" message="Archive it?" confirmLabel="Archive" errorTitle="Could not archive" onConfirm={async () => {}} {...props}
          onCancel={props.onCancel ?? (() => {})} />
      </DaemonProvider>,
    );
  });
}

// Enter confirms a plain confirmation as soon as it opens, which needs focus on its confirm button;
// a destructive one leaves focus on the dialog so a stray Enter does nothing, and one asking for a
// typed word starts in its field. (The unprompted questions are covered where they are asked, in
// `TrustPromptDialog.test.tsx`.)
it.each([
  ["a plain confirmation", {}, "button[type=submit]"],
  ["a destructive confirmation", { destructive: true }, "[role=alertdialog]"],
  ["a typed confirmation", { typeToConfirm: "delete" }, "input"],
])("starts focus in the right place for %s", (_name, props, focused) => {
  mount(props);
  expect(document.activeElement?.matches(focused)).toBe(true);
});

// An unprompted question (a session request whose target was archived) keeps its start on the dialog in its failure
// form, so an Enter typed elsewhere does not acknowledge it; an ordinary confirmation's failure form starts on OK.
it.each([
  [true, "[role=alertdialog]", 0],
  [false, "button", 1],
])("with unprompted %s, its failure form starts on %s and an immediate Enter closes it %s times", (unprompted, focused, closes) => {
  const onCancel = vi.fn();
  mount({ unprompted, failure: failureOf("Gone."), onCancel });
  runOutTimers();
  const active = document.activeElement!;
  expect(active.matches(focused)).toBe(true);
  if (!unprompted) expect(active.textContent).toBe("OK");
  act(() => {
    active.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    active.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true, cancelable: true }));
  });
  expect(onCancel).toHaveBeenCalledTimes(closes);
});

/** An unprompted question that opens already failed, over a dialog that a press without a pointer opened, with focus
 * left on the control that opened it. `raise` shows the question, and `open`, with `deferred`, the dialog under it, for
 * a test to call both in one update, the way a question pushed in the task of the press does. Each frame comes as a 0
 * ms timer, so it runs ahead of any timer set after it was asked for, as a browser may render one before the next timer
 * task. `strict` mounts it all in `StrictMode`, as the app and the gallery are. */
function mountFailedQuestionOverVirtuallyOpenedDialog(strict: boolean, deferred = false): { raise: () => void; open: () => void } {
  framesAsTimers();
  const opener = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => { opener.remove(); });
  opener.focus();
  const { element, open } = openable(<Dialog title="Edit project" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><input aria-label="Name" /></Dialog>, deferred);
  let raise = (): void => {};
  function Host() {
    // The question comes up a commit after what raised it, as the window's questions and alerts do, so that it is not
    // mounted in the commit of the dialog under it, whose own modal then would hide it.
    const [asked, setAsked] = useState(false);
    const [raised, setRaised] = useState(false);
    raise = () => setAsked(true);
    useEffect(() => { if (asked) setRaised(true); }, [asked]);
    return (
      <DaemonProvider value={testDaemon()}>
        {element}
        {raised && <ConfirmDialog unprompted failure={failureOf("Gone.")} title="Start" message="Start it?" confirmLabel="Start" errorTitle="Could not start" onConfirm={async () => {}} onCancel={() => {}} />}
      </DaemonProvider>
    );
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(strict ? <StrictMode><Host /></StrictMode> : <Host />));
  return { raise: () => raise(), open };
}

const failedQuestionHoldsFocus = () => {
  const question = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  expect(question.textContent).toContain("Gone.");
  expect(question.closest('[aria-hidden="true"], [inert]')).toBeNull();
  expect(document.activeElement).toBe(question);
};

// Focusing the failure form as it mounts, before react-aria turns its containment on, makes the dialog underneath,
// which holds no focus of its own yet, move focus to the form's first control, its OK button, where Enter would
// acknowledge it.
describe.each([{ strict: false }, { strict: true }])("an unprompted question that opened already failed over a dialog opened without a pointer (StrictMode: $strict)", ({ strict }) => {
  it("ends on itself, not on OK", async () => {
    const { raise } = mountFailedQuestionOverVirtuallyOpenedDialog(strict);
    act(() => raise());
    await settleFrames();
    failedQuestionHoldsFocus();
  });

  it("ends on itself when raised in the same task as the press that opens the dialog", async () => {
    const { raise, open } = mountFailedQuestionOverVirtuallyOpenedDialog(strict, true);
    act(() => {
      open();
      raise();
    });
    await settleFrames();
    failedQuestionHoldsFocus();
  });

  // react-aria's dialog takes focus off itself half a second after it opens, when focus was not inside it as it
  // mounted, and puts it back once no transition is running on the page. A transition that never reports its end leaves
  // focus on the page body until it ends.
  it("keeps focus on itself while a transition underneath never ends", async () => {
    const { raise } = mountFailedQuestionOverVirtuallyOpenedDialog(strict);
    const name = document.querySelector('input[aria-label="Name"]')!;
    name.dispatchEvent(Object.assign(new Event("transitionrun", { bubbles: true }), { propertyName: "translate" }));
    act(() => raise());
    const pastMountRefocus = trackMountRefocus();
    await settleFrames();
    await pastMountRefocus();
    await settleFrames();
    failedQuestionHoldsFocus();
  });
});

it("shows the localized confirmation example and keeps the literal and editable value LTR", () => {
  mount({ typeToConfirm: "delete" });
  const input = document.body.querySelector("input")!;
  expect(input.placeholder).toBe("Ex. DELETE");
  expect(input.dir).toBe("ltr");
  const chip = document.body.querySelector("label [dir=ltr]")!;
  expect(chip.textContent).toBe("DELETE");
  expect(chip.classList.contains("italic")).toBe(true);
  expect(document.body.querySelector<HTMLButtonElement>("button[type=submit]")!.disabled).toBe(true);
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "delete");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(input.value).toBe("delete");
  expect(document.body.querySelector<HTMLButtonElement>("button[type=submit]")!.disabled).toBe(false);
});

const labels = () => Array.from(document.body.querySelectorAll("[role=alertdialog] button")).map((button) => button.textContent).filter(Boolean);
const closeButton = () => document.body.querySelector("[role=alertdialog] button[aria-label=Close]");

// react-aria puts off a button's `autoFocus` after a press without a pointer (assistive technology, a script) until no
// transition is running anywhere on the page, so one that never reports its end (in Chrome, one still running inside a
// disclosure as it folds away) would leave a confirmation raised by another dialog's confirm button without focus.
// The half-second wait outlasts react-aria's dialog taking focus off itself after mounting.
it("puts focus on Confirm of a confirmation raised over another dialog without a pointer while a transition never ends", async () => {
  framesAsTimers();
  function Stacked() {
    const [asking, setAsking] = useState(false);
    return (
      <Dialog title="Switch account" onClose={() => {}} submitLabel="Switch" onSubmit={() => setAsking(true)}>
        <input aria-label="Name" />
        {asking && <ConfirmDialog title="Start on this account" message="Start anyway?" confirmLabel="Start anyway" hideCancel errorTitle="Could not start" onConfirm={async () => {}} onCancel={() => setAsking(false)} />}
      </Dialog>
    );
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={testDaemon()}><Stacked /></DaemonProvider>));
  await settleFrames();
  const name = document.querySelector('input[aria-label="Name"]')!;
  name.dispatchEvent(Object.assign(new Event("transitionrun", { bubbles: true }), { propertyName: "translate" }));
  onTestFinished(() => { name.dispatchEvent(Object.assign(new Event("transitionend", { bubbles: true }), { propertyName: "translate" })); });
  await act(async () => document.querySelector<HTMLButtonElement>('[role="dialog"] button[type="submit"]')!.click());
  const pastMountRefocus = trackMountRefocus();
  await settleFrames();
  await pastMountRefocus();
  await settleFrames();
  const confirmation = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  expect(confirmation.closest('[aria-hidden="true"], [inert]')).toBeNull();
  expect(document.activeElement).toBe(confirmation.querySelector('button[type="submit"]'));
});

it("shows Cancel and the close button by default", () => {
  mount({});
  expect(labels()).toEqual(["Cancel", "Archive"]);
  expect(closeButton()).not.toBeNull();
});

it("with hideCancel, only Confirm remains, focused, and the close button stays", () => {
  mount({ hideCancel: true });
  expect(labels()).toEqual(["Archive"]);
  expect(document.activeElement?.matches("button[type=submit]")).toBe(true);
  expect(closeButton()).not.toBeNull();
});

it("with hideCancel and Confirm disabled, focus is on the dialog rather than the page", () => {
  mount({ hideCancel: true, confirmDisabled: true });
  expect(document.activeElement?.matches("[role=alertdialog]")).toBe(true);
});

it("titles the alert of a rejected confirmation with the failure title it was given", async () => {
  mount({ errorTitle: "Could not archive the session", onConfirm: () => Promise.reject(new Error("refused")) });
  await act(async () => document.querySelector<HTMLButtonElement>("button[type=submit]")!.click());
  expect(errorAlerts.getSnapshot().map((alert) => alert.title)).toEqual(["Could not archive the session"]);
});

it("keeps the space between the subject and the message in the message's own text node, and none without a subject", () => {
  mount({ subject: <>The thing <b>X</b> goes.</> });
  const sentence = document.body.querySelector("[role=alertdialog] p")!;
  const whitespaceOnly = Array.from(sentence.childNodes).filter((node) => node.nodeType === Node.TEXT_NODE && node.textContent!.trim() === "");
  expect(whitespaceOnly).toEqual([]);
  expect(sentence.textContent).toBe("The thing X goes. Archive it?");
});

it("starts the message without a space when there is no subject", () => {
  mount({});
  expect(document.body.querySelector("[role=alertdialog] p")!.textContent).toBe("Archive it?");
});

it("joins a string message to the subject with no space in Chinese", () => {
  const before = currentLanguage();
  setLanguage("zh-Hans");
  onTestFinished(() => setLanguage(before));
  mount({ subject: "它将被归档。", message: "它将只听从你。" });
  expect(document.body.querySelector("[role=alertdialog] p")!.textContent).toBe("它将被归档。它将只听从你。");
});

// A confirmation that waits for a lookup before its action starts (`waiting`) holds Confirm pending but is not busy:
// the options can still be changed and the question still answered, unlike once `onConfirm` is running.
it("leaves its options operable and its Cancel pressable while it waits for a lookup, and holds them once confirmed", async () => {
  let finish!: () => void;
  const onCancel = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const show = (waiting: boolean) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <ConfirmDialog
          title="Remove"
          message="Remove it?"
          confirmLabel="Remove"
          errorTitle="Could not remove"
          waiting={waiting}
          extraControl={<Checkbox selected={false} onSelectedChange={() => {}}>Trash</Checkbox>}
          onConfirm={() => new Promise<void>((resolve) => { finish = resolve; })}
          onCancel={onCancel}
        />
      </DaemonProvider>,
    );
  });
  const button = (label: string) => [...document.querySelectorAll("button")].find((b) => b.textContent === label)!;
  show(true);
  expect(button("Remove").getAttribute("data-pending")).toBe("true");
  expect(document.querySelector<HTMLInputElement>("input[type=checkbox]")!.disabled).toBe(false);
  expect(button("Cancel").disabled).toBe(false);
  const close = document.querySelector<HTMLButtonElement>('button[aria-label="Close"]');
  expect(close).not.toBeNull();
  expect(close!.disabled).toBe(false);
  act(() => button("Cancel").click());
  expect(onCancel).toHaveBeenCalledOnce();

  show(false);
  await act(async () => void button("Remove").click());
  expect(document.querySelector<HTMLInputElement>("input[type=checkbox]")!.disabled).toBe(true);
  expect(button("Cancel").disabled).toBe(true);
  await act(async () => finish());
});
