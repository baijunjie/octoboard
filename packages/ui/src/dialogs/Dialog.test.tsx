// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { Dialog } from "./Dialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function fakeDaemon(): Daemon {
  return {
    store: createStateStore({}),
    request: vi.fn(),
    toastError: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

/** Renders the frame's own footer — the case `submitLabel` covers — and hands the two buttons to
 * `check`, found by their labels rather than by their type, since `busy` changes the submit button's
 * type. Unmounts whatever the check does, so a failure cannot leave a dialog behind for the next
 * test to read. */
function withFooter(
  props: { busy?: boolean; submitDisabled?: boolean },
  check: (buttons: { submit: HTMLButtonElement; cancel: HTMLButtonElement }) => void,
): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => {
      root.render(
        <DaemonProvider value={fakeDaemon()}>
          <Dialog title="Rename" onClose={() => {}} submitLabel="Save" onSubmit={() => {}} {...props}>
            <span>body</span>
          </Dialog>
        </DaemonProvider>,
      );
    });
    const named = (label: string) =>
      [...container.ownerDocument.querySelectorAll("button")].find((button) => button.textContent === label);
    const submit = named("Save");
    const cancel = named("Cancel");
    if (!submit || !cancel) throw new Error("the frame's own footer did not render both of its buttons");
    check({ submit, cancel });
  } finally {
    act(() => root.unmount());
    container.remove();
  }
}

// Why pending and not disabled: react-aria's `isDisabled` puts the native attribute on the element,
// which blurs the button the user has just pressed, and a dialog whose focus falls to `<body>` loses
// its Escape handling and Tab containment with it. So the submit button has to stay focusable for as
// long as the request runs.
it("marks the submit button pending while busy rather than disabling it", () => {
  withFooter({ busy: true }, ({ submit, cancel }) => {
    expect(submit.hasAttribute("disabled")).toBe(false);
    expect(submit.getAttribute("tabindex")).toBe("0");
    expect(submit.getAttribute("aria-disabled")).toBe("true");
    expect(submit.getAttribute("data-pending")).toBe("true");
    // Cancel is unavailable for the duration of someone else's action, and focus cannot be on it at
    // the moment that action starts, so the native attribute costs nothing here.
    expect(cancel.hasAttribute("disabled")).toBe(true);
  });
});

// `submitDisabled` is the other kind: nothing to submit yet, with focus elsewhere. That is what a
// plain disabled is for, and keeping it one is also what suppresses implicit submission from Enter
// in a text field, since this is the form's only submit control.
it("disables the submit button outright when the dialog has nothing to submit", () => {
  withFooter({ submitDisabled: true }, ({ submit, cancel }) => {
    expect(submit.hasAttribute("disabled")).toBe(true);
    expect(submit.type).toBe("submit");
    expect(cancel.hasAttribute("disabled")).toBe(false);
  });
});
