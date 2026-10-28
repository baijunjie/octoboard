// @vitest-environment jsdom
import { act } from "react";
import { afterEach, describe, expect, it, onTestFinished } from "vitest";

import { focusAfterLoss, setFocusAfterModalLoss } from "../components/regionFocus";
import { errorAlerts } from "../errorAlerts";
import type { SessionRequest, State, TrustPrompt } from "../store";
import { Dialog } from "./Dialog";
import { framesAsTimers, openable, settleFrames } from "./dialogTestSupport";
import { ErrorAlerts } from "./ErrorAlerts";
import { OpensAnotherDialog } from "./nestedDialogTestSupport";
import { mountPendingQuestionDialog } from "./pendingQuestionTestSupport";
import { TextInput } from "./TextInput";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const prompt: TrustPrompt = { session: "s-1", agent: "claude", project: "p-1", path: "/Users/dev/code/shop", trustDir: "/Users/dev/code" };

const request: SessionRequest = {
  requestId: "r-1",
  session: "s-1",
  role: "project",
  console: "c-1",
  project: "p-1",
  agent: "codex",
  branches: [],
  requestedAt: 2,
  action: "start",
};

/** Each question, with the state that raises it and the state that replaces it with another one. */
const questions: [string, Partial<State>, Partial<State>][] = [
  ["a trust prompt", { trustPrompts: [prompt] }, { trustPrompts: [{ ...prompt, session: "s-2", path: "/Users/dev/code/site" }] }],
  ["a session request", { sessionRequests: [request] }, { sessionRequests: [{ ...request, requestId: "r-2", session: "s-2" }] }],
];

const editProject = <Dialog title="Edit project" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><input aria-label="Name" /></Dialog>;

/** Mounts a dialog as opened by a press with `modality`, with focus left on the control that opened it. With `virtual`
 * (a screen reader, a script) the dialog's opening focus waits a frame, and past any transition running on the page.
 * Each frame comes as a 0 ms timer, so it runs ahead of any timer set after it was asked for, as a browser may render
 * one before the next timer task. With `deferred`, the dialog is not opened but `open` opens it, for a test to call in
 * one update with raising a question, the way a question pushed in the task of the press does. */
function mountUnder(modality: "virtual" | "pointer" | "keyboard", strict: boolean, deferred = false) {
  framesAsTimers(modality);
  const opener = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => { opener.remove(); });
  opener.focus();
  const { element, open } = openable(editProject, deferred);
  const dialog = mountPendingQuestionDialog({ homeDir: "/Users/dev" }, () => Promise.resolve({ type: "ack" }), { under: element, strict });
  return { ...dialog, open };
}

const questionHoldsFocus = () => {
  const question = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  expect(question.closest('[aria-hidden="true"], [inert]')).toBeNull();
  expect(document.activeElement).toBe(question);
};

// A question the user did not ask for starts with focus on itself, so an Enter meant for something else answers
// nothing, even over a dialog opened in the same frame, which holds no focus of its own yet: there, focusing the
// question as it mounts, before react-aria turns its containment on, makes the dialog underneath move focus to the
// question's first control, its Close button.
describe.each([{ strict: false }, { strict: true }])("raised over another dialog (StrictMode: $strict)", ({ strict }) => {
  describe.each(questions)("%s", (_question, state) => {
    it("ends on itself over a dialog opened without a pointer", async () => {
      const dialog = mountUnder("virtual", strict);
      act(() => dialog.store.setState(state));
      await settleFrames();
      questionHoldsFocus();
    });

    it("ends on itself when raised in the same task as the press that opens the dialog", async () => {
      const dialog = mountUnder("virtual", strict, true);
      act(() => {
        dialog.open();
        dialog.store.setState(state);
      });
      await settleFrames();
      questionHoldsFocus();
    });

    // react-aria's dialog takes focus off itself half a second after it opens, when focus was not inside it as it
    // mounted, and puts it back once no transition is running on the page. A transition that never reports its end (in
    // Chrome, one still running inside a disclosure as it folds away) leaves focus on the page body until it ends.
    it("keeps focus while a transition underneath never ends", async () => {
      const dialog = mountUnder("virtual", strict);
      const name = document.querySelector('input[aria-label="Name"]')!;
      name.dispatchEvent(Object.assign(new Event("transitionrun", { bubbles: true }), { propertyName: "translate" }));
      act(() => dialog.store.setState(state));
      await settleFrames();
      await act(() => new Promise((resolve) => setTimeout(resolve, 600)));
      await settleFrames();
      questionHoldsFocus();
    });

    it.each(["pointer", "keyboard"] as const)("ends on itself over a dialog opened with a %s press", async (modality) => {
      const dialog = mountUnder(modality, strict);
      act(() => dialog.store.setState(state));
      await settleFrames();
      questionHoldsFocus();
    });
  });
});

// Two modals that each come up a commit after what raised them, raised in one task, mount in one commit: the error
// alert goes on top, the question waiting under it. A dialog that a press opens, and that opens another from an effect as
// it mounts, mounts that one in the commit of a question raised in the press's task: the question goes on top.
describe.each([{ strict: false }, { strict: true }])("(StrictMode: $strict)", ({ strict }) => {
  afterEach(() => { act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }); });

  describe.each(questions)("%s", (_question, state, replaced) => {
    it("raised in the same task as an error alert waits under it, then takes focus once it is acknowledged", async () => {
      framesAsTimers();
      onTestFinished(setFocusAfterModalLoss(() => focusAfterLoss(() => {})));
      const dialog = mountPendingQuestionDialog({ homeDir: "/Users/dev" }, () => Promise.resolve({ type: "ack" }), { after: <ErrorAlerts />, strict });
      act(() => {
        dialog.store.setState(state);
        errorAlerts.report({ message: "Refused" });
      });
      await settleFrames();
      const [question, alert] = Array.from(document.querySelectorAll<HTMLElement>('[role="alertdialog"]'));
      expect(alert.textContent).toContain("Refused");
      expect(alert.closest('[aria-hidden="true"], [inert]')).toBeNull();
      expect(document.activeElement).toBe(alert);
      expect(question.closest('[aria-hidden="true"], [inert]')).not.toBeNull();
      // Read before acknowledged, past react-aria's half-second refocus of a dialog that mounted with focus outside it.
      await act(() => new Promise((resolve) => setTimeout(resolve, 600)));
      act(() => errorAlerts.acknowledge());
      await settleFrames();
      await act(() => new Promise((resolve) => setTimeout(resolve, 450)));
      await settleFrames();
      questionHoldsFocus();
    });

    it("raised in the same task as a press opening a dialog that opens another a commit later ends on itself", async () => {
      framesAsTimers();
      const { element, open } = openable(<OpensAnotherDialog />, true);
      const dialog = mountPendingQuestionDialog({ homeDir: "/Users/dev" }, () => Promise.resolve({ type: "ack" }), { under: element, strict });
      act(() => {
        open();
        dialog.store.setState(state);
      });
      await settleFrames();
      expect(document.querySelectorAll('[role="dialog"]')).toHaveLength(2);
      questionHoldsFocus();
      for (const covered of document.querySelectorAll('[role="dialog"]')) expect(covered.closest('[aria-hidden="true"], [inert]')).not.toBeNull();
    });

    // The question puts focus on itself for each new subject: covered by a dialog opened after it, it must not take
    // focus from that dialog when the daemon replaces what it asks about.
    it("covered by a dialog opened after it, leaves focus there when what it asks about is replaced", async () => {
      framesAsTimers("pointer");
      const { element, open } = openable(<Dialog title="Later" onClose={() => {}} footer={null}><input aria-label="Field" autoFocus /></Dialog>, true);
      const dialog = mountPendingQuestionDialog({ homeDir: "/Users/dev" }, () => Promise.resolve({ type: "ack" }), { after: element, strict });
      act(() => dialog.store.setState(state));
      await settleFrames();
      questionHoldsFocus();
      act(() => open());
      await settleFrames();
      const later = document.querySelector<HTMLElement>('[role="dialog"]')!;
      const field = later.querySelector("input")!;
      act(() => field.focus());
      act(() => dialog.store.setState(replaced));
      await settleFrames();
      expect(document.querySelector('[role="alertdialog"]')!.closest('[aria-hidden="true"], [inert]')).not.toBeNull();
      expect(later.contains(document.activeElement)).toBe(true);
    });
  });
});

// A question raised in the same task as a press without a pointer that opens a dialog takes focus before that dialog's
// own opening focus, which react-aria then drops, so closing the question gives focus back to nothing: the press left it
// on `<body>`. It goes where that dialog's opening focus would have gone, as after an alert acknowledged there.
describe.each([{ strict: false }, { strict: true }])("closed over a dialog its press opened in the same task (StrictMode: $strict)", ({ strict }) => {
  const formDialog = <Dialog title="Open session" onClose={() => {}} submitLabel="Open" onSubmit={() => {}}><TextInput label="Name" value="" onChange={() => {}} autoFocus /></Dialog>;
  const closeQuestion = async (store: { setState: (state: Partial<State>) => void }) => {
    // Read before answered, past react-aria's half-second refocus of a dialog that mounted with focus outside it.
    await act(() => new Promise((resolve) => setTimeout(resolve, 600)));
    act(() => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
    // A refused session request leaves once the daemon says so.
    act(() => store.setState({ trustPrompts: [], sessionRequests: [] }));
    await settleFrames();
    await act(() => new Promise((resolve) => setTimeout(resolve, 450)));
    await settleFrames();
  };

  describe.each(questions)("%s", (_question, state) => {
    it("hands focus to that dialog's opening control", async () => {
      framesAsTimers();
      onTestFinished(setFocusAfterModalLoss(() => focusAfterLoss(() => {})));
      (document.activeElement as HTMLElement | null)?.blur();
      const { element, open } = openable(formDialog, true);
      const dialog = mountPendingQuestionDialog({ homeDir: "/Users/dev" }, () => Promise.resolve({ type: "ack" }), { under: element, strict });
      act(() => {
        open();
        dialog.store.setState(state);
      });
      await settleFrames();
      questionHoldsFocus();
      await closeQuestion(dialog.store);
      expect(dialog.onScreen()).toBe(false);
      expect(document.activeElement).toBe(document.querySelector('[role="dialog"] input'));
    });

    it("gives focus back to the control that had it when the dialog had settled", async () => {
      framesAsTimers();
      onTestFinished(setFocusAfterModalLoss(() => focusAfterLoss(() => {})));
      const dialog = mountPendingQuestionDialog({ homeDir: "/Users/dev" }, () => Promise.resolve({ type: "ack" }), { under: formDialog, strict });
      await settleFrames();
      const cancel = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find((button) => button.textContent === "Cancel")!;
      act(() => cancel.focus());
      act(() => dialog.store.setState(state));
      await settleFrames();
      questionHoldsFocus();
      await closeQuestion(dialog.store);
      expect(dialog.onScreen()).toBe(false);
      expect(document.activeElement).toBe(cancel);
    });
  });
});
