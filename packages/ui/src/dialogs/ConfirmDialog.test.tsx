// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { currentLanguage, setLanguage } from "../i18n/language";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";
import { framesAsTimers, settleFrames } from "./dialogTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function fakeDaemon(): Daemon {
  return {
    store: createStateStore({}),
    request: vi.fn(),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

/** Renders a confirmation and takes it down when the test finishes. */
function mount(props: { errorTitle?: string; onConfirm?: () => Promise<void>; destructive?: boolean; typeToConfirm?: string; hideCancel?: boolean; confirmDisabled?: boolean; subject?: React.ReactNode; message?: string }): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => {
    root.render(
      <DaemonProvider value={fakeDaemon()}>
        <ConfirmDialog title="Archive" message="Archive it?" confirmLabel="Archive" errorTitle="Could not archive" onConfirm={async () => {}} {...props}
          onCancel={() => {}} />
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
  act(() => root.render(<DaemonProvider value={fakeDaemon()}><Stacked /></DaemonProvider>));
  await settleFrames();
  const name = document.querySelector('input[aria-label="Name"]')!;
  name.dispatchEvent(Object.assign(new Event("transitionrun", { bubbles: true }), { propertyName: "translate" }));
  onTestFinished(() => { name.dispatchEvent(Object.assign(new Event("transitionend", { bubbles: true }), { propertyName: "translate" })); });
  await act(async () => document.querySelector<HTMLButtonElement>('[role="dialog"] button[type="submit"]')!.click());
  await settleFrames();
  await act(() => new Promise((resolve) => setTimeout(resolve, 600)));
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
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
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
