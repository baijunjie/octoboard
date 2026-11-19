// @vitest-environment jsdom
import { act, StrictMode, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { DaemonProvider } from "../../store";
import { testDaemon } from "../../test/testDaemon";
import { errorAlerts, type AcknowledgementView } from "../alerts/errorAlerts";
import { CodedError } from "../../errors/codedError";
import { MODAL_CLOSE_SETTLE_MS, setFocusAfterModalLoss } from "../focus/modalFocus";
import { Dialog } from "./Dialog";
import { useDialogAction } from "./dialogHooks";
import { failureOf, framesAsTimers, settleFrames, trackMountRefocus, withModality } from "./dialogTestSupport";
import { fakeTimers, runOutTimers } from "../../test/clock";
import { TextInput } from "../form/TextInput";
import { Button, Checkbox, Input, Switch } from "../../primitives";
import { ExternalLink } from "../text/ExternalLink";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// react-aria reads how focus last arrived from a page-wide state that a virtual click of an earlier test leaves behind,
// and a dialog opened under it waits a frame for its opening focus: start each test as after a pointer.
beforeEach(() => {
  act(() => setInteractionModality("pointer"));
});

/** Renders the frame's own footer — the case `submitLabel` covers — and hands the two buttons to
 * `check`, found by their labels rather than by their type, since `busy` changes the submit button's
 * type. Unmounts whatever the check does, so a failure cannot leave a dialog behind for the next
 * test to read. */
function withFooter(
  props: { busy?: boolean; submitDisabled?: boolean; autoFocusSubmit?: boolean },
  check: (buttons: { submit: HTMLButtonElement; cancel: HTMLButtonElement }) => void,
): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => {
      root.render(
        <DaemonProvider value={testDaemon()}>
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

/** Renders a dialog that opens on its submit button and hands `check` a `show`, which renders it again with
 * `submitDisabled` as given, and `named`, which finds a footer button by its label. Unmounts whatever the
 * check does. */
function withTogglableSubmit(
  check: (dialog: { show: (submitDisabled: boolean) => void; named: (label: string) => HTMLButtonElement }) => void,
): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const show = (submitDisabled: boolean) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Hand off" onClose={() => {}} submitLabel="Save" submitDisabled={submitDisabled} autoFocusSubmit onSubmit={() => {}}>
          <span>body</span>
        </Dialog>
      </DaemonProvider>,
    );
  });
  const named = (label: string) => [...document.querySelectorAll("button")].find((button) => button.textContent === label)!;
  try {
    check({ show, named });
  } finally {
    act(() => root.unmount());
    container.remove();
  }
}

// A dialog's surface is `--overlay` whatever its size, which HeroUI's `--default` fills are not tuned
// for, so every dialog takes the app's own (`.control-fills` in `style.css`).
it("gives every dialog the fills of the `--default` family made for its surface", () => {
  withFooter({}, () => {
    expect(document.querySelector("[role=dialog]")?.classList.contains("control-fills")).toBe(true);
  });
});

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

it("disables the close button while busy, with the frame's own footer or a footer of the dialog's own", () => {
  const closeButton = () => document.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!;
  withFooter({ busy: true }, () => expect(closeButton().disabled).toBe(true));
  withFooter({}, () => expect(closeButton().disabled).toBe(false));

  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    act(() => {
      root.render(
        <DaemonProvider value={testDaemon()}>
          <Dialog title="Import" onClose={() => {}} busy footer={<span>own footer</span>}>
            <span>body</span>
          </Dialog>
        </DaemonProvider>,
      );
    });
    expect(closeButton().disabled).toBe(true);
  } finally {
    act(() => root.unmount());
    container.remove();
  }
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

// A natively disabled button cannot keep focus, and focus off the dialog takes its Escape and Tab with it.
it("opens on Cancel rather than on a submit button that has nothing to submit", () => {
  withFooter({ autoFocusSubmit: true, submitDisabled: true }, ({ submit, cancel }) => {
    expect(document.activeElement).toBe(cancel);
    expect(submit.disabled).toBe(true);
  });
  withFooter({ autoFocusSubmit: true }, ({ submit }) => expect(document.activeElement).toBe(submit));
});

// A press without a pointer (an assistive technology's, a script's `click()`) on a control that takes no focus when
// pressed leaves focus on the page body as the dialog opens, and react-aria puts the field's `autoFocus` off to a
// frame, dropping it if focus has moved by then. Each frame comes as a 0 ms timer, so it runs ahead of any timer set
// after it, as a browser may render one before the next timer task. StrictMode, as the app runs, mounts every effect a
// second time.
it.each([{ strict: false }, { strict: true }])(
  "opens with focus in its first text field when a press without a pointer opened it (StrictMode: $strict)",
  async ({ strict }) => {
    framesAsTimers();
    (document.activeElement as HTMLElement | null)?.blur();
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    const dialog = (
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Rename" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}>
          <TextInput label="Name" value="" onChange={() => {}} autoFocus />
        </Dialog>
      </DaemonProvider>
    );
    act(() => root.render(strict ? <StrictMode>{dialog}</StrictMode> : dialog));
    await settleFrames();
    const field = document.querySelector('[role="dialog"] input');
    expect(field).not.toBeNull();
    expect(document.activeElement).toBe(field);
  },
);

it("moves focus from the submit button to Cancel when the submit becomes disabled under it", () => {
  withTogglableSubmit(({ show, named }) => {
    show(false);
    expect(document.activeElement).toBe(named("Save"));
    show(true);
    expect(document.activeElement).toBe(named("Cancel"));
  });
});

it("moves focus to Cancel when the browser has already blurred the disabled submit button to the body", () => {
  withTogglableSubmit(({ show, named }) => {
    show(false);
    const submit = named("Save");
    expect(document.activeElement).toBe(submit);
    // WebKit blurs a button to <body> as it becomes disabled, before the layout effect runs. jsdom will not
    // blur a disabled element, so the blur is raised by hand right after the attribute is set, and the button
    // hidden as the active element.
    const active = Object.getOwnPropertyDescriptor(Document.prototype, "activeElement")!.get!;
    let blurred = false;
    Object.defineProperty(document, "activeElement", { configurable: true, get: () => (blurred && active.call(document) === submit ? document.body : active.call(document)) });
    const setAttribute = Element.prototype.setAttribute;
    const spy = vi.spyOn(Element.prototype, "setAttribute").mockImplementation(function (this: Element, name: string, value: string) {
      setAttribute.call(this, name, value);
      if (this !== submit || name !== "disabled") return;
      blurred = true;
      submit.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    });
    onTestFinished(() => {
      spy.mockRestore();
      delete (document as { activeElement?: unknown }).activeElement;
    });
    show(true);
    expect(submit.disabled).toBe(true);
    expect(document.activeElement).toBe(named("Cancel"));
  });
});

it("leaves focus alone when the submit button is disabled while the user is elsewhere", () => {
  withTogglableSubmit(({ show, named }) => {
    show(false);
    act(() => named("Save").blur());
    show(true);
    expect(document.activeElement).toBe(document.body);
  });
});

it.each([
  ["modal", false, false],
  ["modal form", false, true],
  ["alert dialog", true, false],
  ["alert dialog form", true, true],
])("preserves HeroUI's header, body and footer spacing selectors in a %s", (_name, alert, form) => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const submit = vi.fn();
  try {
    render(root, (
      <Dialog title="Rename" alert={alert} onClose={() => {}} footer={<button type="submit">Save</button>}
        onSubmit={form ? submit : undefined}>
        <input aria-label="Name" />
      </Dialog>
    ));
    const prefix = alert ? "alert-dialog" : "modal";
    const body = document.querySelector(`.${prefix}__header + .${prefix}__body`);
    expect(body).not.toBeNull();
    expect(body?.nextElementSibling?.classList.contains(`${prefix}__footer`)).toBe(true);
    if (form) {
      const button = document.querySelector<HTMLButtonElement>("button[type=submit]");
      expect(button?.form?.contains(body!)).toBe(true);
      expect(button?.form?.querySelector("input")?.form).toBe(button?.form);
      act(() => button?.click());
      expect(submit).toHaveBeenCalledTimes(1);
    }
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});

const clearance = () => document.documentElement.style.getPropertyValue("--dialog-footer-clearance");

const render = (root: Root, element: React.ReactElement) =>
  act(() => {
    root.render(<DaemonProvider value={testDaemon()}>{element}</DaemonProvider>);
  });

/** Mounts `element` with each footer measuring the horizontal extent `extent` gives for its dialog
 * (found by its title) and 80px tall at the window's bottom, hands the root to `check`, and cleans
 * up whatever `check` does. */
async function withMeasuredFooters(
  element: React.ReactElement,
  extent: (title: string) => { left: number; right: number; top?: number; bottom?: number },
  check: (root: Root) => Promise<void>,
): Promise<void> {
  const region = document.body.appendChild(document.createElement("div"));
  region.dataset.slot = "toast-region";
  const toast = region.appendChild(document.createElement("div"));
  toast.dataset.slot = "toast";
  toast.dataset.frontmost = "true";
  toast.style.setProperty("--toast-height", "64px");
  const spy = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.slot !== "modal-footer") return { top: 0, left: 0, right: 0 } as DOMRect;
    const title = this.closest("[role=dialog]")?.querySelector("h2, [slot=title]")?.textContent ?? "";
    return { top: window.innerHeight - 80, bottom: window.innerHeight, ...extent(title) } as DOMRect;
  });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  // The measuring waits for the next frame after each change, which `settleFrames` runs.
  framesAsTimers("pointer");
  try {
    render(root, element);
    await settleFrames();
    await check(root);
  } finally {
    act(() => root.unmount());
    container.remove();
    spy.mockRestore();
    region.remove();
  }
}

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

// A centred dialog can reach across the toast column without reaching down to the toast itself.
// Only its footer's real intersection matters, including for a tall or expanded toast stack.
it.each([
  ["below the footer", 600, 660, 64, false],
  ["overlapping the footer", 670, 730, 100, true],
  ["above the footer", 756, 768, 16, false],
])("keeps a toast at its ordinary position when %s", async (_name, top, bottom, height, lifted) => {
  await withMeasuredFooters(
    <Dialog title="Processes" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><span>body</span></Dialog>,
    () => ({ ...wide, top, bottom }),
    async () => {
      const toast = document.querySelector<HTMLElement>('[data-slot="toast"]')!;
      toast.style.setProperty("--toast-height", `${height}px`);
      await settleFrames();
      expect(clearance()).toBe(lifted ? `${window.innerHeight - top}px` : "");
    },
  );
});

it("remeasures when a toast arrives, grows or disappears without changing the dialog", async () => {
  await withMeasuredFooters(
    <Dialog title="Processes" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><span>body</span></Dialog>,
    () => ({ ...wide, top: 600, bottom: 660 }),
    async () => {
      const region = document.querySelector<HTMLElement>('[data-slot="toast-region"]')!;
      const toast = region.firstElementChild as HTMLElement;
      expect(clearance()).toBe("");
      toast.style.setProperty("--toast-height", "120px");
      await settleFrames();
      expect(clearance()).toBe("168px");
      // Repeated measurements while lifted must still use the ordinary bottom position.
      window.dispatchEvent(new Event("resize"));
      await settleFrames();
      expect(clearance()).toBe("168px");
      toast.style.setProperty("--toast-height", "64px");
      await settleFrames();
      expect(clearance()).toBe("");
      toast.remove();
      await settleFrames();
      expect(clearance()).toBe("");
      toast.style.setProperty("--toast-height", "120px");
      region.append(toast);
      await settleFrames();
      expect(clearance()).toBe("168px");
      toast.remove();
      await settleFrames();
      expect(clearance()).toBe("");
    },
  );
});

it("keeps the same clearance while a toast stack expands and collapses", async () => {
  const height = window.innerHeight;
  window.innerHeight = 800;
  try {
    await withMeasuredFooters(
      <Dialog title="Processes" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><span>body</span></Dialog>,
      () => ({ ...wide, top: 530, bottom: 566 }),
      async () => {
        const region = document.querySelector<HTMLElement>('[data-slot="toast-region"]')!;
        const front = region.firstElementChild as HTMLElement;
        front.style.setProperty("--toast-height", "48px");
        const older = front.cloneNode() as HTMLElement;
        older.dataset.frontmost = "false";
        older.style.setProperty("--toast-height", "164px");
        older.style.setProperty("--front-height", "48px");
        older.style.setProperty("--offset-collapsed", "12px");
        older.style.setProperty("--offset-expanded", "60px");
        region.append(older);
        for (const expanded of [false, true, false]) {
          front.dataset.expanded = older.dataset.expanded = String(expanded);
          // Force a fresh measurement in both states, independent of the observers' filters.
          window.dispatchEvent(new Event("resize"));
          await settleFrames();
          expect(clearance()).toBe("270px");
        }
      },
    );
  } finally {
    window.innerHeight = height;
  }
});

it("remeasures the ordinary toast position above connection and update strips", async () => {
  await withMeasuredFooters(
    <Dialog title="Processes" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><span>body</span></Dialog>,
    () => ({ ...wide, top: 600, bottom: 660 }),
    async () => {
      const style = document.documentElement.style;
      try {
        expect(clearance()).toBe("");
        style.setProperty("--connection-banner-height", "48px");
        await settleFrames();
        expect(clearance()).toBe("168px");
        style.setProperty("--update-notice-height", "160px");
        await settleFrames();
        expect(clearance()).toBe("");
        style.removeProperty("--update-notice-height");
        await settleFrames();
        expect(clearance()).toBe("168px");
        style.removeProperty("--connection-banner-height");
        await settleFrames();
        expect(clearance()).toBe("");
      } finally {
        style.removeProperty("--connection-banner-height");
        style.removeProperty("--update-notice-height");
      }
    },
  );
});

it("mirrors the footer intersection in a right-to-left document", async () => {
  const computed = window.getComputedStyle;
  const spy = vi.spyOn(window, "getComputedStyle").mockImplementation((element, pseudo) => {
    const style = computed(element, pseudo);
    return element === document.documentElement ? new Proxy(style, { get: (target, key) => key === "direction" ? "rtl" : Reflect.get(target, key) }) : style;
  });
  try {
    await withMeasuredFooters(
      <Dialog title="Processes" onClose={() => {}} submitLabel="Save" onSubmit={() => {}}><span>body</span></Dialog>,
      () => ({ left: 100, right: 500 }),
      async () => expect(clearance()).toBe("80px"),
    );
  } finally {
    spy.mockRestore();
  }
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
      await settleFrames();
      expect(clearance()).toBe("");
      render(root, outer(false));
      await settleFrames();
      expect(clearance()).toBe("80px");
    },
  );
});

/** Mounts a dialog with `props` (`alert` for the `AlertDialog` frame), puts keyboard focus on
 * `control` the way a keyboard user does — a key press, then the focus — presses Escape there, and
 * answers how many times the dialog asked to close. Focusing after a key press is what opens a
 * tooltip, so a control that has one has it open. */
function escapeFrom(
  control: (dialog: Element) => HTMLElement | null | undefined,
  props: { alert?: boolean; keyboardDismissHeld?: boolean; busy?: boolean; body?: React.ReactNode } = {},
): { closes: number; tooltip: boolean } {
  const { body = <span>body</span>, ...dialogProps } = props;
  const onClose = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    render(
      root,
      <Dialog title="Rename" onClose={onClose} submitLabel="Save" onSubmit={() => {}} {...dialogProps}>
        {body}
      </Dialog>,
    );
    const dialog = document.querySelector("[role=dialog], [role=alertdialog]");
    const focused = dialog && control(dialog);
    if (!focused) throw new Error("the dialog did not render the control to focus");
    act(() => {
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
      focused.focus();
    });
    const tooltip = document.querySelector("[role=tooltip]") !== null;
    act(() => {
      focused.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    });
    return { closes: onClose.mock.calls.length, tooltip };
  } finally {
    act(() => root.unmount());
    container.remove();
  }
}

const closeTrigger = (dialog: Element) => dialog.querySelector<HTMLElement>("[data-slot$=close-trigger]");

it.each([
  ["modal", {}, 1],
  ["confirmation", { alert: true }, 0],
  ["failure modal", { failing: true }, 0],
  ["modal whose keyboard dismissal is held", { keyboardDismissHeld: true }, 1],
  ["busy modal", { busy: true }, 0],
])("handles a backdrop press on a %s", (_name, props, closes) => {
  const onClose = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  render(root, <Dialog title="Confirm" onClose={onClose} footer={null} {...props}><span>body</span></Dialog>);
  const backdrop = document.querySelector<HTMLElement>("[data-slot$=-backdrop]");
  if (!backdrop) throw new Error("the dialog did not render a backdrop");
  act(() => {
    backdrop.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }));
    backdrop.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0 }));
    backdrop.click();
  });
  expect(onClose).toHaveBeenCalledTimes(closes);
});

it("still closes from its close button while the keyboard dismissal is held", () => {
  const onClose = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  render(root, <Dialog title="Held" onClose={onClose} keyboardDismissHeld footer={null}><span>body</span></Dialog>);
  act(() => closeTrigger(document.querySelector("[role=dialog]")!)!.click());
  expect(onClose).toHaveBeenCalledTimes(1);
});

// The close button carries a tooltip, and a tooltip opens on keyboard focus. react-aria then holds
// Escape at `document` to dismiss the tooltip, above `<body>`, where React dispatches the modal's
// own Escape handling from — so the dialog has to close that press itself. Both frames put a close
// button in the same place, so both lose the key there.
it.each([
  ["modal", {}],
  ["alert dialog", { alert: true }],
])("closes a %s on Escape with focus on its close button, whose tooltip is open there", (_frame, props) => {
  expect(escapeFrom(closeTrigger, props)).toEqual({ closes: 1, tooltip: true });
});

// And closes exactly once from a control with no tooltip, where react-aria's own handling arrives.
it("closes on Escape once from a control without a tooltip", () => {
  expect(
    escapeFrom((dialog) => [...dialog.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent === "Cancel")),
  ).toEqual({ closes: 1, tooltip: false });
});

// `keyboardDismissHeld` is for a dialog whose content closes something of its own on Escape: the key closes
// the dialog neither where react-aria handles it nor where a tooltip made the dialog handle it itself, while
// the close button and a press outside still dismiss (the backdrop cases, and "still closes from its close
// button while the keyboard dismissal is held").
it("does not close on Escape while the keyboard dismissal is held, with a tooltip open or not", () => {
  expect(escapeFrom(closeTrigger, { keyboardDismissHeld: true })).toEqual({ closes: 0, tooltip: true });
  expect(
    escapeFrom((dialog) => [...dialog.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent === "Cancel"), { keyboardDismissHeld: true }),
  ).toEqual({ closes: 0, tooltip: false });
});

// A busy dialog cannot be left: Escape is ignored where react-aria handles it and where a tooltip made the dialog
// handle it itself. The pending submit button stays focusable, so the key arrives from there, and so does a pending
// body button with a tooltip, where the dialog handles the key; every other control is disabled and cannot take focus.
it("does not close on Escape while busy, with a tooltip open or not", () => {
  const submit = (dialog: Element) => [...dialog.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent?.includes("Save"));
  expect(escapeFrom(submit, { busy: true }).closes).toBe(0);
  expect(escapeFrom(submit, { busy: true, alert: true }).closes).toBe(0);
  expect(escapeFrom(submit).closes).toBe(1);

  const body = (pending: boolean) => (
    <Button tooltip="Explained" pending={pending}>Extra</Button>
  );
  const extra = (dialog: Element) => [...dialog.querySelectorAll<HTMLElement>("button")].find((b) => b.textContent === "Extra");
  expect(escapeFrom(extra, { busy: true, body: body(true) })).toEqual({ closes: 0, tooltip: true });
  expect(escapeFrom(extra, { body: body(false) })).toEqual({ closes: 1, tooltip: true });
});

/** Renders a dialog whose body measures `scrollHeight` tall in a `clientHeight` box (jsdom has no
 * layout), and hands the body's scroll container to `check`. */
async function withScroller(sizes: { scrollHeight: number; clientHeight: number }, check: (scroller: HTMLElement) => void): Promise<void> {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const restore = (["scrollHeight", "clientHeight"] as const).map((key) => {
    const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, key);
    Object.defineProperty(HTMLElement.prototype, key, { configurable: true, get: () => sizes[key] });
    return () => (original ? Object.defineProperty(HTMLElement.prototype, key, original) : delete (HTMLElement.prototype as unknown as Record<string, unknown>)[key]);
  });
  try {
    act(() => {
      root.render(
        <DaemonProvider value={testDaemon()}>
          <Dialog title="Long" onClose={() => {}} footer={null}>
            <span>body</span>
          </Dialog>
        </DaemonProvider>,
      );
    });
    // HeroUI writes the fade's state on the next frame.
    runOutTimers();
    const scroller = document.querySelector<HTMLElement>("[role=dialog] [data-slot=scroll-shadow]");
    if (!scroller) throw new Error("the dialog's body is not inside a scroll container with a fade");
    check(scroller);
  } finally {
    act(() => root.unmount());
    container.remove();
    restore.forEach((undo) => undo());
  }
}

// Every dialog's content can outgrow the window, and the fade at the edge is what tells the user there
// is more: it belongs to the frame so that no dialog has to remember it.
it("holds every dialog's body in a scroll container that fades at an edge it can still scroll towards", async () => {
  await withScroller({ scrollHeight: 1000, clientHeight: 100 }, (scroller) => {
    expect(scroller.textContent).toBe("body");
    expect(scroller.classList.contains("scroll-shadow--fade")).toBe(true);
    expect(scroller.getAttribute("data-bottom-scroll")).toBe("true");
  });
});

// A focused control is scrolled to the edge of the container and no further, which is under the fade.
it("keeps a control scrolled into view clear of the fade at the edge", async () => {
  await withScroller({ scrollHeight: 1000, clientHeight: 100 }, (scroller) => {
    const fade = Number(scroller.getAttribute("data-scroll-shadow-size"));
    expect(fade).toBeGreaterThan(0);
    expect(parseFloat(scroller.style.scrollPaddingBlock)).toBeGreaterThanOrEqual(fade);
  });
});

// With every control in the footer, nothing in the body takes focus, so the keys that scroll need the
// scroller itself to.
it("makes a scrolling body a named region the keyboard can focus, and a body that fits not one", async () => {
  await withScroller({ scrollHeight: 1000, clientHeight: 100 }, (scroller) => {
    expect(scroller.tabIndex).toBe(0);
    expect(scroller.getAttribute("role")).toBe("region");
    const dialog = document.querySelector("[role=dialog]");
    expect(scroller.getAttribute("aria-labelledby")).toBe(dialog?.getAttribute("aria-labelledby"));
    expect(document.getElementById(scroller.getAttribute("aria-labelledby") ?? "")?.textContent).toBe("Long");
  });
  await withScroller({ scrollHeight: 100, clientHeight: 100 }, (scroller) => {
    expect(scroller.hasAttribute("tabindex")).toBe(false);
    expect(scroller.hasAttribute("role")).toBe(false);
  });
});

it("shows no fade when the body fits", async () => {
  await withScroller({ scrollHeight: 100, clientHeight: 100 }, (scroller) => {
    expect(scroller.getAttribute("data-bottom-scroll")).not.toBe("true");
    expect(scroller.getAttribute("data-top-scroll")).not.toBe("true");
    expect(scroller.hasAttribute("data-top-bottom-scroll")).toBe(false);
  });
});

// The Enter or Space that opened a dialog (a menu item acts on keydown) can still be held down when
// the dialog's confirm button takes focus; its repeats must not submit what the user has not read.
it.each(["Enter", " "])("ignores a repeated %j on the submit button", (key) => {
  withFooter({}, ({ submit }) => {
    const repeat = new KeyboardEvent("keydown", { key, repeat: true, bubbles: true, cancelable: true });
    const first = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
    act(() => submit.dispatchEvent(repeat));
    act(() => submit.dispatchEvent(first));
    expect(repeat.defaultPrevented).toBe(true);
    expect(first.defaultPrevented).toBe(false);
  });
});

it("eases the dialog's height when its content changes it, but not for its first size", () => {
  const observers: ResizeObserverCallback[] = [];
  vi.stubGlobal("ResizeObserver", class {
    constructor(callback: ResizeObserverCallback) { observers.push(callback); }
    observe() {}
    disconnect() {}
  });
  const animate = vi.fn(() => ({ playState: "finished", cancel() {} }));
  Element.prototype.animate = animate as never;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
    delete (Element.prototype as Partial<Element>).animate;
  });
  act(() => root.render(
    <DaemonProvider value={testDaemon()}>
      <Dialog title="Rename" onClose={() => {}} footer={null}><span>body</span></Dialog>
    </DaemonProvider>,
  ));
  const resize = (blockSize: number) => observers.forEach((report) => report([{ borderBoxSize: [{ blockSize }], contentRect: { height: blockSize } } as unknown as ResizeObserverEntry], {} as ResizeObserver));
  resize(200);
  expect(animate).not.toHaveBeenCalled();
  resize(260);
  expect(animate).toHaveBeenCalledWith([{ height: "200px" }, { height: "260px" }], expect.objectContaining({ duration: 200 }));
});

it("reports a failed dialog action to the acknowledgement queue, with no inline alert, and settles busy", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let action!: ReturnType<typeof useDialogAction>;
  function Subject() {
    action = useDialogAction(undefined, "Could not save");
    return null;
  }
  try {
    act(() => root.render(<Subject />));
    await act(() => action.run(() => Promise.reject(new Error("Something failed"))));
    expect(errorAlerts.getSnapshot().some((error) => error.message === "Something failed" && error.title === "Could not save")).toBe(true);
    expect(action.busy).toBe(false);
    expect(container.querySelector('[role="alert"]')).toBeNull();
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});

it("titles a failed dialog action with the dialog's title even where the failure's code has a title of its own", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let action!: ReturnType<typeof useDialogAction>;
  function Subject() {
    action = useDialogAction(undefined, "Could not remove the branches");
    return null;
  }
  try {
    act(() => root.render(<Subject />));
    await act(() => action.run(() => Promise.reject(new CodedError("Not removed.", "session_branches_not_removed", {}))));
    expect(errorAlerts.getSnapshot()[0].title).toBe("Could not remove the branches");
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});

// Having picked from a select, the user presses Enter to go ahead: it submits from a control that holds
// a choice, but a button that acts keeps it, and nothing is submitted while the submit button is disabled or pending.
it.each([
  ["a closed select", '<button type="button" aria-haspopup="listbox" aria-expanded="false">Agent</button>', {}, 1],
  ["an open select", '<button type="button" aria-haspopup="listbox" aria-expanded="true">Agent</button>', {}, 0],
  ["a button that acts", '<button type="button">Advanced</button>', {}, 0],
  ["a closed select with nothing to submit", '<button type="button" aria-haspopup="listbox" aria-expanded="false">Agent</button>', { submitDisabled: true }, 0],
  ["a closed select while busy", '<button type="button" aria-haspopup="listbox" aria-expanded="false">Agent</button>', { busy: true }, 0],
])("Enter on %s", (_name, html, props, submits) => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const submit = vi.fn();
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  render(root, (
    <Dialog title="Open session" onClose={() => {}} submitLabel="Open" onSubmit={submit} {...props}>
      <div dangerouslySetInnerHTML={{ __html: html }} />
    </Dialog>
  ));
  const control = [...document.querySelectorAll("button")].find((b) => b.textContent === "Agent" || b.textContent === "Advanced");
  expect(control).toBeDefined();
  act(() => control?.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })));
  expect(submit).toHaveBeenCalledTimes(submits);
});

// A failure that ends the dialog's purpose replaces its content in the same modal, in the error alert dialog's form.
it("turns into a failure modal in place: danger icon, title, explanation, details, the acknowledgement alone in its footer, and a close button", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const onClose = vi.fn();
  const failure = failureOf("Gone.", <code>raw detail</code>);
  const render = (withFailure: boolean) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Rename" onClose={onClose} submitLabel="Save" onSubmit={() => {}} failure={withFailure ? failure : undefined}>
          <span>body</span>
        </Dialog>
      </DaemonProvider>,
    );
  });
  try {
    render(false);
    const before = document.querySelector("[role=dialog]");
    expect(before?.querySelector("form")).not.toBeNull();
    render(true);
    const dialog = document.querySelector("[role=alertdialog]");
    // The same modal, with its form, title and body gone.
    expect(dialog).toBe(before);
    expect(dialog?.querySelector("form")).toBeNull();
    expect(dialog?.textContent).not.toContain("Rename");
    expect(dialog?.querySelector("h2, h1, [data-slot$=heading]")?.textContent).toBe("Error");
    expect(dialog?.querySelector('[data-slot$="icon"]')).not.toBeNull();
    expect(dialog?.textContent).toContain("Gone.");
    expect(dialog?.querySelector("details summary")?.textContent).toBe("Error details");
    const footer = [...(dialog?.querySelectorAll("button") ?? [])].filter((button) => button.closest("[data-slot$=footer]")).map((button) => button.textContent);
    expect(footer).toEqual(["OK"]);
    // Escape and a press outside are held; the close button does what OK does.
    const backdrop = document.querySelector<HTMLElement>("[data-slot$=-backdrop]")!;
    act(() => {
      backdrop.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, button: 0 }));
      backdrop.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0 }));
      backdrop.click();
      dialog!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
    });
    expect(onClose).not.toHaveBeenCalled();
    act(() => closeTrigger(dialog!)!.click());
    expect(onClose).toHaveBeenCalledOnce();
    act(() => [...dialog!.querySelectorAll("button")].find((button) => button.textContent === "OK")!.click());
    expect(onClose).toHaveBeenCalledTimes(2);
  } finally {
    act(() => root.unmount());
    container.remove();
  }
});

/** Renders a dialog with a field that may turn into its failure form, on a fake clock started before mounting.
 * `show` renders it again with the given failure (or none); the dialog is unmounted when the test finishes. */
function mountFailable(options: { focusesItselfOnOpen: boolean; failure?: AcknowledgementView; onClose?: () => void }) {
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const show = (failure?: AcknowledgementView, resetKey?: string) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Rename" onClose={options.onClose ?? (() => {})} submitLabel="Save" onSubmit={() => {}} focusesItselfOnOpen={options.focusesItselfOnOpen} failure={failure} resetKey={resetKey}>
          <input aria-label="Name" />
        </Dialog>
      </DaemonProvider>,
    );
  });
  show(options.failure);
  return { show };
}

const alertDialog = () => document.querySelector<HTMLElement>("[role=alertdialog]")!;
const closeButton = () => alertDialog().querySelector<HTMLButtonElement>('button[aria-label="Close"]')!;
const okButton = () => [...alertDialog().querySelectorAll("button")].find((button) => button.textContent === "OK")!;
/** An Enter typed on whatever holds focus, as a key press and release. */
const typeEnter = () => act(() => {
  const target = document.activeElement ?? document.body;
  target.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
  target.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true, cancelable: true }));
});

// A question the user did not ask for starts on the dialog itself, so an Enter typed elsewhere does nothing; that holds
// for its failure form too, while any other dialog's failure form opens on OK.
it.each([
  [true, "starts on the dialog itself and ignores an Enter typed right away"],
  [false, "starts on OK, which an Enter acknowledges"],
])("with focusesItselfOnOpen %s, a dialog that opens already failed %s", (focusesItselfOnOpen) => {
  const onClose = vi.fn();
  mountFailable({ focusesItselfOnOpen, failure: failureOf("Gone."), onClose });
  runOutTimers();
  // The close button comes before OK, so the dialog itself is not simply the first control of an unprompted form.
  expect(closeButton()).not.toBeNull();
  expect(document.activeElement).toBe(focusesItselfOnOpen ? alertDialog() : okButton());
  typeEnter();
  expect(onClose).toHaveBeenCalledTimes(focusesItselfOnOpen ? 0 : 1);
});

it("moves focus from a field to the dialog itself, not OK, when an unprompted dialog turns into its failure form", () => {
  const onClose = vi.fn();
  const { show } = mountFailable({ focusesItselfOnOpen: true, onClose });
  runOutTimers();
  act(() => document.querySelector<HTMLElement>('input[aria-label="Name"]')!.focus());
  show(failureOf("Gone."));
  runOutTimers();
  expect(document.activeElement).toBe(alertDialog());
  typeEnter();
  expect(onClose).not.toHaveBeenCalled();
});

it("puts focus on OK when an ordinary dialog turns into its failure form", () => {
  const { show } = mountFailable({ focusesItselfOnOpen: false });
  runOutTimers();
  act(() => document.querySelector<HTMLElement>('input[aria-label="Name"]')!.focus());
  show(failureOf("Gone."));
  runOutTimers();
  expect(document.activeElement).toBe(okButton());
});

it("returns focus to the dialog itself when an unprompted failure form changes to another failure", () => {
  const onClose = vi.fn();
  const { show } = mountFailable({ focusesItselfOnOpen: true, failure: failureOf("Gone."), onClose });
  runOutTimers();
  act(() => okButton().focus());
  show(failureOf("Gone for another reason."));
  runOutTimers();
  expect(alertDialog().textContent).toContain("Gone for another reason.");
  expect(document.activeElement).toBe(alertDialog());
  typeEnter();
  expect(onClose).not.toHaveBeenCalled();
});

// A queued question that has failed too, shown once the previous one is gone, is a new subject: it starts on the dialog
// again even when its failure reads the same, so the Enter that acknowledged the previous one cannot repeat onto OK.
it("returns focus to the dialog itself when an unprompted failure form moves on to another subject with the same failure", () => {
  const onClose = vi.fn();
  const { show } = mountFailable({ focusesItselfOnOpen: true, onClose });
  show(failureOf("Gone."), "first");
  runOutTimers();
  act(() => okButton().focus());
  show(failureOf("Gone."), "second");
  runOutTimers();
  expect(document.activeElement).toBe(alertDialog());
  typeEnter();
  expect(onClose).not.toHaveBeenCalled();
});

// An unprompted failure form that appears while focus is already on the dialog (it turned failed in place, or moved on
// to another queued subject) passes focus through a hidden element inside the dialog and back, so the dialog is focused
// anew and a screen reader reads it again; nothing else would tell it the subject changed.
it.each([
  ["turns failed in place", (show: (failure?: AcknowledgementView, resetKey?: string) => void) => show(failureOf("Gone."), "one"), undefined],
  ["moves on to another queued subject", (show: (failure?: AcknowledgementView, resetKey?: string) => void) => show(failureOf("Gone."), "two"), failureOf("Gone.")],
])("focuses the dialog anew, without Enter acting, when an unprompted dialog already focused %s", (_case, change, before) => {
  const onClose = vi.fn();
  const { show } = mountFailable({ focusesItselfOnOpen: true, onClose });
  if (before) show(before, "one");
  runOutTimers();
  act(() => document.querySelector<HTMLElement>("[role=dialog], [role=alertdialog]")!.focus());
  const dialog = document.querySelector<HTMLElement>("[role=dialog], [role=alertdialog]")!;
  expect(document.activeElement).toBe(dialog);
  let focused = 0;
  const count = (event: FocusEvent) => { if (event.target === dialog) focused += 1; };
  dialog.addEventListener("focusin", count);
  onTestFinished(() => dialog.removeEventListener("focusin", count));
  change(show);
  runOutTimers();
  expect(document.querySelector("[role=alertdialog]")).toBe(dialog);
  expect(focused).toBe(1);
  expect(document.activeElement).toBe(dialog);
  typeEnter();
  expect(onClose).not.toHaveBeenCalled();
});

// OK taking focus in a dialog that a failure turned up in, or that moved on to another failed subject, is a move the
// user did not make, so its ring follows the last input: after a key it shows, after a mouse press it does not. As the
// dialog opens already failed it is the opening focus, whose ring follows react-aria's modality, not the last input.
// The modality is put back to `pointer` after the input, as a screen reader's virtual press leaves it, so only the ring
// rule could set it, and it is read as focus arrives, before anything else of react-aria's resets it.
it.each([
  ["turns failed while open", "key", true],
  ["turns failed while open", "mouse", false],
  ["moves on to another failed subject", "key", true],
  ["opens already failed", "key", false],
] as const)("when it %s after a %s, OK has focus and its ring is %s", (scenario, input, ringed) => {
  withModality(input === "key" ? "keyboard" : "pointer");
  act(() => setInteractionModality("pointer"));
  let modalityAtFocus: ReturnType<typeof getInteractionModality>;
  const record = (event: FocusEvent) => {
    if ((event.target as HTMLElement).textContent === "OK") modalityAtFocus = getInteractionModality();
  };
  window.addEventListener("focusin", record, true);
  onTestFinished(() => window.removeEventListener("focusin", record, true));
  const { show } = mountFailable({ focusesItselfOnOpen: false, failure: scenario === "opens already failed" ? failureOf("Gone.") : undefined });
  if (scenario === "moves on to another failed subject") {
    show(failureOf("Gone."), "first");
    runOutTimers();
    act(() => (document.activeElement as HTMLElement).blur());
  }
  runOutTimers();
  act(() => setInteractionModality("pointer"));
  if (scenario === "turns failed while open") show(failureOf("Gone."));
  if (scenario === "moves on to another failed subject") show(failureOf("Gone."), "second");
  runOutTimers();
  const ok = okButton();
  expect(document.activeElement).toBe(ok);
  expect(modalityAtFocus!).toBe(ringed ? "keyboard" : "pointer");
  expect(ok.hasAttribute("data-focus-visible")).toBe(ringed);
});

// react-aria puts off a button's `autoFocus` after a press without a pointer, and the frame's own refocus then moves
// focus to the dialog, so a failure form that relied on it ended on the dialog. It focuses OK itself, from a layout
// effect.
describe.each([{ strict: false }, { strict: true }])("an ordinary dialog opened without a pointer (StrictMode: $strict)", ({ strict }) => {
  it.each([
    ["turns failed while open", false],
    ["opens already failed", true],
  ])("puts focus on OK when it %s", async (_case, failedAtOpen) => {
    framesAsTimers();
    const opener = document.body.appendChild(document.createElement("button"));
    onTestFinished(() => { opener.remove(); });
    opener.focus();
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    const show = (failure?: AcknowledgementView) => act(() => {
      const tree = (
        <DaemonProvider value={testDaemon()}>
          <Dialog title="Rename" onClose={() => {}} submitLabel="Save" onSubmit={() => {}} failure={failure}>
            <input aria-label="Name" />
          </Dialog>
        </DaemonProvider>
      );
      root.render(strict ? <StrictMode>{tree}</StrictMode> : tree);
    });
    show(failedAtOpen ? failureOf("Gone.") : undefined);
    const pastMountRefocus = trackMountRefocus();
    await settleFrames();
    if (!failedAtOpen) {
      // Focus on a field that the failure form takes away falls to `<body>`, where the frame's refocus acts.
      act(() => document.querySelector<HTMLElement>('input[aria-label="Name"]')!.focus());
      show(failureOf("Gone."));
    }
    await settleFrames();
    // Past react-aria's half-second refocus of a dialog that mounted with focus outside it.
    await pastMountRefocus();
    await settleFrames();
    const ok = [...document.querySelectorAll<HTMLButtonElement>("[role=alertdialog] button")].find((button) => button.textContent === "OK")!;
    expect(ok.closest('[aria-hidden="true"], [inert]')).toBeNull();
    expect(document.activeElement).toBe(ok);
  });
});

// A failure that ends the dialog's purpose often takes the control that opened the dialog with it, so react-aria's
// restore finds nothing and leaves focus on `<body>`; acknowledging the failure form hands that on to the window.
/** Mounts a failed dialog that goes away when acknowledged, over a control that may hold focus after it, and
 * registers `fallback` as the window's fallback for focus left on `<body>`. */
function acknowledgeFailure(fallback: () => void, onClose: () => void, by: "OK" | "Close" = "OK") {
  onTestFinished(setFocusAfterModalLoss(fallback));
  function Host() {
    const [open, setOpen] = useState(true);
    return (
      <>
        <button type="button">Elsewhere</button>
        {open && <Dialog title="Rename" onClose={() => { onClose(); setOpen(false); }} focusesItselfOnOpen submitLabel="Save" onSubmit={() => {}} failure={failureOf("Gone.")}><span>body</span></Dialog>}
      </>
    );
  }
  fakeTimers();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => root.render(<DaemonProvider value={testDaemon()}><Host /></DaemonProvider>));
  runOutTimers();
  act(() => (by === "OK" ? okButton() : closeButton()).click());
}

it.each(["OK", "Close"] as const)("hands focus to the window's fallback after acknowledging the failure form with %s when focus is left on the page body", (by) => {
  const fallback = vi.fn();
  const onClose = vi.fn();
  acknowledgeFailure(fallback, onClose, by);
  expect(onClose).toHaveBeenCalledOnce();
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
  (document.activeElement as HTMLElement | null)?.blur();
  expect(fallback).not.toHaveBeenCalled();
  act(() => void vi.advanceTimersByTime(MODAL_CLOSE_SETTLE_MS));
  expect(fallback).toHaveBeenCalledOnce();
});

it("leaves focus where it landed after acknowledging the failure form, handing nothing to the fallback", () => {
  const fallback = vi.fn();
  acknowledgeFailure(fallback, () => {});
  const elsewhere = [...document.querySelectorAll("button")].find((button) => button.textContent === "Elsewhere")!;
  act(() => elsewhere.focus());
  act(() => void vi.advanceTimersByTime(MODAL_CLOSE_SETTLE_MS));
  expect(document.activeElement).toBe(elsewhere);
  expect(fallback).not.toHaveBeenCalled();
});

it("sets the foreground colour on the body of every frame, so prose does not inherit HeroUI's muted grey", () => {
  for (const props of [{}, { alert: true }, { size: "viewer" as const }]) {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    try {
      act(() => {
        root.render(
          <DaemonProvider value={testDaemon()}>
            <Dialog title="Notice" onClose={() => {}} footer={null} {...props}>
              <span>body</span>
            </Dialog>
          </DaemonProvider>,
        );
      });
      const body = [...document.querySelectorAll("span")].find((span) => span.textContent === "body")?.closest('[data-slot$="body"]');
      expect(body?.classList.contains("text-foreground"), JSON.stringify(props)).toBe(true);
    } finally {
      act(() => root.unmount());
      container.remove();
    }
  }
});

/** A dialog with a field, a switch, a checkbox, a link and a button of its own in its body, rendered `busy` or not,
 * with the frame's own footer or a footer of its own. `show` renders it again. */
function withBodyControls(
  props: { busy: boolean; footer?: React.ReactNode },
  check: (dialog: { show: (props: { busy: boolean; footer?: React.ReactNode }) => void }) => void,
): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const show = (next: { busy: boolean; footer?: React.ReactNode }) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Edit" onClose={() => {}} {...(next.footer === undefined ? { submitLabel: "Save", onSubmit: () => {} } : { footer: next.footer })} busy={next.busy}>
          <Input label="Name" value="" onChange={() => {}} />
          <Switch label="Share" selected={false} onSelectedChange={() => {}} />
          <Checkbox selected={false} onSelectedChange={() => {}}>Include</Checkbox>
          <ExternalLink href="https://example.com/guide">Guide</ExternalLink>
          <Button>Elsewhere</Button>
        </Dialog>
      </DaemonProvider>,
    );
  });
  try {
    show(props);
    check({ show });
  } finally {
    act(() => root.unmount());
    container.remove();
  }
}

const bodyControls = () => ({
  input: document.querySelector<HTMLInputElement>('input[aria-label="Name"]')!,
  share: document.querySelector<HTMLInputElement>("input[role=switch]")!,
  checkbox: document.querySelector<HTMLInputElement>("input[type=checkbox]")!,
  link: [...document.querySelectorAll<HTMLElement>("a")].find((a) => a.textContent === "Guide")!,
  elsewhere: [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Elsewhere")!,
});

// The whole body is held while a request runs, not only the buttons of the footer: a field edited or a switch flipped
// under a request that is already out would change nothing of it. Assistive technology still reads them, so they are
// disabled, never `inert`.
it("disables every control and link of the body while busy, which stay in the page for assistive technology", () => {
  withBodyControls({ busy: true }, () => {
    const { input, share, checkbox, link, elsewhere } = bodyControls();
    expect([input.disabled, share.disabled, checkbox.disabled, elsewhere.disabled]).toEqual([true, true, true, true]);
    // A link with no destination is not followed and takes no focus, and is still read as a link, unavailable.
    expect(link.hasAttribute("href")).toBe(false);
    expect(link.hasAttribute("tabindex")).toBe(false);
    act(() => link.focus());
    expect(document.activeElement).not.toBe(link);
    expect(link.getAttribute("role")).toBe("link");
    expect(link.getAttribute("aria-disabled")).toBe("true");
    expect(document.querySelector("[inert]")).toBeNull();
  });
  withBodyControls({ busy: false }, () => {
    const { input, share, checkbox, link, elsewhere } = bodyControls();
    expect([input.disabled, share.disabled, checkbox.disabled, elsewhere.disabled]).toEqual([false, false, false, false]);
    expect(link.getAttribute("href")).toBe("https://example.com/guide");
  });
});

it("keeps the pending submit button enabled and in the tab order while the rest of a busy dialog is disabled", () => {
  withBodyControls({ busy: true }, () => {
    const submit = [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Save")!;
    expect(submit.disabled).toBe(false);
    expect(submit.getAttribute("data-pending")).toBe("true");
    expect(submit.tabIndex).toBe(0);
  });
});

// A natively disabled control cannot keep focus, and a dialog whose focus falls to `<body>` loses its Escape and Tab.
it("moves focus from a body control that busy disables to the pending submit button, or to the dialog with none", () => {
  withBodyControls({ busy: false }, ({ show }) => {
    const { input } = bodyControls();
    act(() => input.focus());
    expect(document.activeElement).toBe(input);
    show({ busy: true });
    expect(document.activeElement).toBe([...document.querySelectorAll("button")].find((b) => b.textContent === "Save"));
  });
  withBodyControls({ busy: false, footer: <span>own footer</span> }, ({ show }) => {
    const { elsewhere } = bodyControls();
    act(() => elsewhere.focus());
    show({ busy: true, footer: <span>own footer</span> });
    expect(document.activeElement).toBe(document.querySelector("[role=dialog]"));
  });
});

/** Makes `document` behave as in a browser that blurs `element` to `<body>` as an attribute change disables it, which
 * jsdom never does (it leaves a disabled element focused, and treats an anchor without `href` as unfocusable, so no
 * blur of one can be real): each time `change` says an attribute of `element` was just set (`"set"`) or removed
 * (`"remove"`) the event is raised and `element` is hidden as the active element, right after the attribute has changed
 * and before React's layout effects run. React 19 sets `disabled` with `setAttribute`, and a link's `href` goes with
 * `removeAttribute`. `unmarked` names an attribute that is left off `element` while the event is raised, as where the
 * engine blurs it before that attribute is set. Returns what puts it back. */
function blurToBodyOnChange(
  element: Element,
  change: (operation: "set" | "remove", name: string) => boolean,
  unmarked?: string,
): () => void {
  const active = Object.getOwnPropertyDescriptor(Document.prototype, "activeElement")!.get!;
  let blurred = false;
  Object.defineProperty(document, "activeElement", { configurable: true, get: () => (blurred && active.call(document) === element ? document.body : active.call(document)) });
  const setAttribute = Element.prototype.setAttribute;
  const removeAttribute = Element.prototype.removeAttribute;
  const blur = () => {
    blurred = true;
    const mark = unmarked === undefined ? null : element.getAttribute(unmarked);
    if (mark !== null) removeAttribute.call(element, unmarked!);
    element.dispatchEvent(new FocusEvent("focusout", { bubbles: true }));
    if (mark !== null) setAttribute.call(element, unmarked!, mark);
  };
  const setSpy = vi.spyOn(Element.prototype, "setAttribute").mockImplementation(function (this: Element, name: string, value: string) {
    setAttribute.call(this, name, value);
    if (this === element && change("set", name)) blur();
  });
  const removeSpy = vi.spyOn(Element.prototype, "removeAttribute").mockImplementation(function (this: Element, name: string) {
    removeAttribute.call(this, name);
    if (this === element && change("remove", name)) blur();
  });
  return () => {
    setSpy.mockRestore();
    removeSpy.mockRestore();
    delete (document as { activeElement?: unknown }).activeElement;
  };
}

const savingButton = () => [...document.querySelectorAll("button")].find((b) => b.textContent === "Save");

it("hands focus on even where the browser has already blurred the control to the body as it was disabled", () => {
  withBodyControls({ busy: false }, ({ show }) => {
    const { input } = bodyControls();
    act(() => input.focus());
    // WebKit blurs a control as it becomes disabled, before the layout effects run; jsdom never does.
    const restore = blurToBodyOnChange(input, (operation, name) => operation === "set" && name === "disabled");
    try {
      show({ busy: true });
      expect(input.disabled).toBe(true);
      expect(document.activeElement).toBe(savingButton());
    } finally {
      restore();
    }
  });
});

it("moves focus from a link that loses its destination, even where the browser blurs it to the body as it does", () => {
  withBodyControls({ busy: false }, ({ show }) => {
    const { link } = bodyControls();
    act(() => link.focus());
    expect(document.activeElement).toBe(link);
    show({ busy: true });
    expect(document.activeElement).toBe(savingButton());
  });
  withBodyControls({ busy: false }, ({ show }) => {
    const { link } = bodyControls();
    act(() => link.focus());
    // An anchor that loses its `href` is not `:disabled`, and an engine may blur it to `<body>` as it does.
    const restore = blurToBodyOnChange(link, (operation, name) => operation === "remove" && name === "href");
    try {
      show({ busy: true });
      expect(link.hasAttribute("href")).toBe(false);
      expect(document.activeElement).toBe(savingButton());
    } finally {
      restore();
    }
  });
});

it("moves focus from a link blurred to the body before it is marked disabled, as a link that lost its href is", () => {
  withBodyControls({ busy: false }, ({ show }) => {
    const { link } = bodyControls();
    act(() => link.focus());
    // Nothing yet says the link is disabled when the blur arrives; only that it has no destination.
    const restore = blurToBodyOnChange(link, (operation, name) => operation === "remove" && name === "href", "aria-disabled");
    try {
      show({ busy: true });
      expect(link.getAttribute("aria-disabled")).toBe("true");
      expect(document.activeElement).toBe(savingButton());
    } finally {
      restore();
    }
  });
});

it("keeps focus on a pending button of the footer rather than moving it to another one", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const show = (busy: boolean) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Edit" onClose={() => {}} busy={busy} footer={<><Button pending={busy}>First</Button><Button pending={busy}>Second</Button></>}>
          <span>body</span>
        </Dialog>
      </DaemonProvider>,
    );
  });
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  show(false);
  const second = [...document.querySelectorAll("button")].find((b) => b.textContent === "Second")!;
  act(() => second.focus());
  show(true);
  expect(document.activeElement).toBe(second);
});

it("moves focus to the dialog when the pending submit button becomes disabled while busy, as Cancel is disabled too", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const show = (submitDisabled: boolean) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Edit" onClose={() => {}} submitLabel="Save" busy submitDisabled={submitDisabled} autoFocusSubmit onSubmit={() => {}}>
          <span>body</span>
        </Dialog>
      </DaemonProvider>,
    );
  });
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  show(false);
  const save = savingButton()!;
  act(() => save.focus());
  expect(document.activeElement).toBe(save);
  show(true);
  expect(save.disabled).toBe(true);
  expect(document.activeElement).toBe(document.querySelector("[role=dialog]"));
});

/** A dialog with a field whose submit starts a request the test settles. `fail` rejects it, first opening an alert
 * dialog over it where `withAlert` says so, as the error alert does (it is raised before the dialog stops being busy,
 * and takes focus); `acknowledge` closes that alert, which gives focus back to what had it when it opened. */
function withFailingRequest(check: (dialog: { field: () => HTMLInputElement; save: () => HTMLButtonElement; fail: (withAlert: boolean) => Promise<void>; acknowledge: () => Promise<void> }) => Promise<void>): Promise<void> {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  framesAsTimers();
  let rejectRequest!: (error: Error) => void;
  let showAlert!: (shown: boolean) => void;
  function Subject() {
    const { busy, run } = useDialogAction(undefined, "Could not save");
    const [alerting, setAlerting] = useState(false);
    showAlert = setAlerting;
    return (
      <>
        <Dialog title="Edit" onClose={() => {}} submitLabel="Save" busy={busy} onSubmit={() => void run(() => new Promise<void>((_, reject) => { rejectRequest = reject; }))}>
          <Input label="Name" value="" onChange={() => {}} />
        </Dialog>
        {alerting && <Dialog alert title="Could not save" onClose={() => setAlerting(false)} footer={<Button onPress={() => setAlerting(false)}>Acknowledge</Button>}><span>failed</span></Dialog>}
      </>
    );
  }
  act(() => root.render(<DaemonProvider value={testDaemon()}><Subject /></DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const dialogButton = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === label)!;
  return check({
    field: () => document.querySelector<HTMLInputElement>('input[aria-label="Name"]')!,
    save: () => dialogButton("Save"),
    fail: async (withAlert) => {
      if (withAlert) act(() => showAlert(true));
      await settleFrames();
      await act(async () => rejectRequest(new Error("Something failed")));
      await settleFrames();
    },
    acknowledge: async () => {
      act(() => dialogButton("Acknowledge").click());
      await settleFrames();
    },
  });
}

it("returns focus to the field it was on when a failed request leaves the dialog, once its error alert is acknowledged", () =>
  withFailingRequest(async ({ field, save, fail, acknowledge }) => {
    act(() => field().focus());
    act(() => save().click());
    expect(field().disabled).toBe(true);
    expect(document.activeElement).toBe(save());
    await fail(true);
    // The alert holds focus: the dialog does not take it.
    expect(document.activeElement?.closest("[role=alertdialog]")).not.toBeNull();
    expect(field().disabled).toBe(false);
    await acknowledge();
    expect(document.activeElement).toBe(field());
  }));

it("returns focus to the field right away when a failed request raises no alert over the dialog", () =>
  withFailingRequest(async ({ field, save, fail }) => {
    act(() => field().focus());
    act(() => save().click());
    expect(document.activeElement).toBe(save());
    await fail(false);
    expect(document.activeElement).toBe(field());
  }));

/** A dialog with a field and, unless `named` is false, a named return target (`returnFocusRef`, the field): `start`
 * makes it busy, `end` stops it being busy, with the submit button disabled where `refuse` says so (as a refusal that
 * asks again does), which moves focus to Cancel in the same commit. `refuseWhileBusy` disables the submit button while
 * busy instead, which moves focus to the dialog, and `removeField` takes the field out. `start` returns a button outside
 * the dialog, standing for an error alert. */
function withNamedReturn(check: (dialog: { field: () => HTMLInputElement; save: () => HTMLButtonElement; end: (refuse: boolean) => void; start: () => HTMLElement; refuseWhileBusy: () => void; removeField: () => void }) => void, named = true): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  framesAsTimers();
  const returnFocusRef = { current: null as HTMLElement | null };
  let hasField = true;
  const show = (busy: boolean, refuse: boolean) => act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <Dialog title="Edit" onClose={() => {}} submitLabel="Save" busy={busy} submitDisabled={refuse} returnFocusRef={named ? returnFocusRef : undefined} onSubmit={() => {}}>
          {hasField && <Input label="Name" value="" onChange={() => {}} />}
        </Dialog>
      </DaemonProvider>,
    );
  });
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  show(false, false);
  const field = () => document.querySelector<HTMLInputElement>('input[aria-label="Name"]')!;
  const outside = container.appendChild(document.createElement("button"));
  check({
    field,
    save: () => savingButton() as HTMLButtonElement,
    end: (refuse) => show(false, refuse),
    start: () => {
      if (named) returnFocusRef.current = field();
      show(true, false);
      return outside;
    },
    refuseWhileBusy: () => show(true, true),
    removeField: () => {
      hasField = false;
    },
  });
}

it("returns focus to the named element at once when busy ends with focus moved to Cancel in the same commit", () => {
  withNamedReturn(({ field, save, end, start }) => {
    act(() => save().focus());
    start();
    expect(document.activeElement).toBe(savingButton());
    end(true);
    expect(document.activeElement).toBe(field());
  });
});

it("returns focus at once when busy ends on <body>, and does not hold the return for a later focus", () => {
  withNamedReturn(({ field, save, end, start }) => {
    act(() => save().focus());
    const outside = start();
    act(() => (document.activeElement as HTMLElement).blur());
    expect(document.activeElement).toBe(document.body);
    end(false);
    expect(document.activeElement).toBe(field());
    act(() => outside.focus());
    act(() => save().focus());
    expect(document.activeElement).toBe(save());
  });
});

it("returns focus to the remembered field when busy ends with the submit button disabled in the same commit", () => {
  withNamedReturn(({ field, end, start }) => {
    act(() => field().focus());
    start();
    end(true);
    expect(document.activeElement).toBe(field());
  }, false);
});

it("returns focus to the remembered field when the submit button was disabled while busy, which moved focus to the dialog", () => {
  withNamedReturn(({ field, end, start, refuseWhileBusy }) => {
    act(() => field().focus());
    start();
    refuseWhileBusy();
    expect(document.activeElement).not.toBe(field());
    end(false);
    expect(document.activeElement).toBe(field());
  }, false);
});

it("does not return focus at busy end after a Tab inside the dialog while busy", () => {
  withNamedReturn(({ field, end, start }) => {
    act(() => field().focus());
    start();
    act(() => savingButton()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true })));
    end(false);
    expect(document.activeElement).toBe(savingButton());
  }, false);
});

it("puts focus on the pending button, not the page, when busy ends on <body> and the control to return to is gone", () => {
  withNamedReturn(({ field, end, start, removeField }) => {
    act(() => field().focus());
    start();
    act(() => (document.activeElement as HTMLElement).blur());
    expect(document.activeElement).toBe(document.body);
    removeField();
    end(false);
    expect(document.activeElement).toBe(savingButton());
  }, false);
});

it("does not redirect a press inside the dialog while the return is held back for an alert", () => {
  withNamedReturn(({ field, save, end, start }) => {
    act(() => save().focus());
    const outside = start();
    act(() => outside.focus());
    end(false);
    expect(document.activeElement).toBe(outside);
    act(() => save().dispatchEvent(new MouseEvent("pointerdown", { bubbles: true })));
    act(() => save().focus());
    expect(document.activeElement).toBe(save());
    expect(document.activeElement).not.toBe(field());
  });
});

it("forgets the control focus was taken from when the dialog is reused for another subject", () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  framesAsTimers();
  let moveTo!: (subject: string) => void;
  function Subject() {
    const [subject, setSubject] = useState("one");
    moveTo = setSubject;
    const { busy, run } = useDialogAction(subject, "Could not save");
    return (
      <Dialog title="Edit" resetKey={subject} onClose={() => {}} submitLabel="Save" busy={busy} onSubmit={() => void run(() => new Promise<void>(() => {}))}>
        <Input label="Name" value="" onChange={() => {}} />
      </Dialog>
    );
  }
  act(() => root.render(<DaemonProvider value={testDaemon()}><Subject /></DaemonProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const field = document.querySelector<HTMLInputElement>('input[aria-label="Name"]')!;
  act(() => field.focus());
  act(() => savingButton()!.click());
  expect(field.disabled).toBe(true);
  expect(document.activeElement).toBe(savingButton());
  // The request is still out for the old subject, but the dialog is no longer busy for the new one.
  act(() => moveTo("two"));
  expect(field.disabled).toBe(false);
  expect(document.activeElement).toBe(savingButton());
});

describe("a dialog's footerStart", () => {
  /** Renders a dialog with the frame's own footer unless `footer` is given, and returns what it finds in it. */
  function mountFooter(props: { footerStart?: React.ReactNode; footer?: React.ReactNode }) {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => {
      root.render(
        <DaemonProvider value={testDaemon()}>
          {props.footer === undefined ? (
            <Dialog title="Memory" onClose={() => {}} submitLabel="Save" onSubmit={() => {}} footerStart={props.footerStart}>
              <span>body</span>
            </Dialog>
          ) : (
            <Dialog title="Memory" onClose={() => {}} footer={props.footer} footerStart={props.footerStart}>
              <span>body</span>
            </Dialog>
          )}
        </DaemonProvider>,
      );
    });
    const named = (label: string) => [...document.querySelectorAll("button")].find((button) => button.textContent === label);
    const status = [...document.querySelectorAll("span, p, div")].find((element) => element.children.length === 0 && element.textContent === "3 of 200 entries");
    return { named, status };
  }

  it("shows the status first in the footer, before the frame's own buttons", () => {
    const { named, status } = mountFooter({ footerStart: <span>3 of 200 entries</span> });
    const cancel = named("Cancel")!;
    expect(status).toBeDefined();
    const footer = cancel.parentElement!;
    expect(status!.closest("div")!.parentElement).toBe(footer);
    expect(footer.firstElementChild).toBe(status!.closest("div"));
    expect(status!.compareDocumentPosition(cancel) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(status!.compareDocumentPosition(named("Save")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("shows the status before a footer of the dialog's own too", () => {
    const { named, status } = mountFooter({ footerStart: <span>3 of 200 entries</span>, footer: <button type="button">Own</button> });
    expect(status).toBeDefined();
    expect(status!.compareDocumentPosition(named("Own")!) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it.each([["undefined", undefined], ["null", null], ["false", false]])("adds nothing to the footer for %s", (_name, footerStart) => {
    const { named } = mountFooter({ footerStart });
    expect(named("Cancel")!.parentElement!.firstElementChild).toBe(named("Cancel"));
  });

  it("shows no status where the dialog has no footer", () => {
    const { status } = mountFooter({ footerStart: <span>3 of 200 entries</span>, footer: null });
    expect(status).toBeUndefined();
  });
});
