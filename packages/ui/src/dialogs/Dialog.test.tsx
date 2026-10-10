// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { expect, it, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { Dialog } from "./Dialog";

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

const clearance = () => document.documentElement.style.getPropertyValue("--dialog-footer-clearance");

const render = (root: Root, element: React.ReactElement) =>
  act(() => {
    root.render(<DaemonProvider value={fakeDaemon()}>{element}</DaemonProvider>);
  });

/** Mounts `element` with each footer measuring the horizontal extent `extent` gives for its dialog
 * (found by its title) and 80px tall at the window's bottom, hands the root to `check`, and cleans
 * up whatever `check` does. */
async function withMeasuredFooters(
  element: React.ReactElement,
  extent: (title: string) => { left: number; right: number },
  check: (root: Root) => Promise<void>,
): Promise<void> {
  const spy = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.slot !== "modal-footer") return { top: 0, left: 0, right: 0 } as DOMRect;
    const title = this.closest("[role=dialog]")?.querySelector("h2, [slot=title]")?.textContent ?? "";
    return { top: window.innerHeight - 80, ...extent(title) } as DOMRect;
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    render(root, element);
    await frame();
    await check(root);
  } finally {
    act(() => root.unmount());
    container.remove();
    spy.mockRestore();
  }
}

const frame = () => act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));

const wide = { left: 16, right: 1008 };
const centred = { left: 200, right: 500 };

// The toasts sit in the window's bottom end corner: a dialog whose footer reaches it (the file
// viewer's Previous and Next) lifts them clear of the footer while it is open and drops them back
// once it closes, while a small centred dialog leaves them alone.
it("lifts the toasts above a footer that reaches their corner and drops them once it closes", async () => {
  await withMeasuredFooters(
    <Dialog title="File" size="viewer" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}>
      <span>body</span>
    </Dialog>,
    () => wide,
    async (root) => {
      expect(clearance()).toBe("80px");
      render(root, <span />);
      expect(clearance()).toBe("");
    },
  );
});

it("leaves the toasts alone for a centred dialog clear of their corner", async () => {
  await withMeasuredFooters(
    <Dialog title="Rename" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}>
      <span>body</span>
    </Dialog>,
    () => centred,
    async () => expect(clearance()).toBe(""),
  );
});

// In a window narrower than the toast breakpoint the toasts are at the top, so no footer is in
// their way, however wide it is.
it("leaves the toasts alone in a narrow window, where they sit at the top", async () => {
  const width = window.innerWidth;
  window.innerWidth = 390;
  try {
    await withMeasuredFooters(
      <Dialog title="File" size="viewer" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}>
        <span>body</span>
      </Dialog>,
      () => ({ left: 0, right: 390 }),
      async () => expect(clearance()).toBe(""),
    );
  } finally {
    window.innerWidth = width;
  }
});

it("measures the outer dialog again when a dialog nested in it closes", async () => {
  const outer = (inner: boolean) => (
    <Dialog title="Outer" size="viewer" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}>
      {inner ? (
        <Dialog title="Inner" onClose={() => {}} submitLabel="Pick" onSubmit={() => {}}>
          <span>inner</span>
        </Dialog>
      ) : (
        <span>outer</span>
      )}
    </Dialog>
  );
  await withMeasuredFooters(
    outer(false),
    (title) => (title === "Inner" ? centred : wide),
    async (root) => {
      expect(clearance()).toBe("80px");
      render(root, outer(true));
      await frame();
      expect(clearance()).toBe("");
      render(root, outer(false));
      await frame();
      expect(clearance()).toBe("80px");
    },
  );
});
