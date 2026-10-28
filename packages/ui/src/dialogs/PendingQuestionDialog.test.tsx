// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, onTestFinished } from "vitest";

import type { SessionRequest, State, TrustPrompt } from "../store";
import { Dialog } from "./Dialog";
import { framesAsTimers, openable, settleFrames } from "./dialogTestSupport";
import { mountPendingQuestionDialog } from "./pendingQuestionTestSupport";

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

const questions: [string, Partial<State>][] = [
  ["a trust prompt", { trustPrompts: [prompt] }],
  ["a session request", { sessionRequests: [request] }],
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
