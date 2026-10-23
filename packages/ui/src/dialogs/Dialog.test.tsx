// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { errorAlerts } from "../errorAlerts";
import { Dialog, useDialogAction } from "./Dialog";

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
  props: { busy?: boolean; submitDisabled?: boolean; autoFocusSubmit?: boolean },
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
      <DaemonProvider value={fakeDaemon()}>
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

/** Mounts a dialog with `props` (`alert` for the `AlertDialog` frame), puts keyboard focus on
 * `control` the way a keyboard user does — a key press, then the focus — presses Escape there, and
 * answers how many times the dialog asked to close. Focusing after a key press is what opens a
 * tooltip, so a control that has one has it open. */
function escapeFrom(
  control: (dialog: Element) => HTMLElement | null | undefined,
  props: { alert?: boolean } = {},
): { closes: number; tooltip: boolean } {
  const onClose = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  try {
    render(
      root,
      <Dialog title="Rename" onClose={onClose} submitLabel="Save" onSubmit={() => {}} {...props}>
        <span>body</span>
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
  ["acknowledgement", { alert: true, dismissable: false }, 0],
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

/** Renders a dialog whose body measures `scrollHeight` tall in a `clientHeight` box (jsdom has no
 * layout), and hands the body's scroll container to `check`. */
async function withScroller(sizes: { scrollHeight: number; clientHeight: number }, check: (scroller: HTMLElement) => void): Promise<void> {
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
        <DaemonProvider value={fakeDaemon()}>
          <Dialog title="Long" onClose={() => {}} footer={null}>
            <span>body</span>
          </Dialog>
        </DaemonProvider>,
      );
    });
    // HeroUI writes the fade's state on the next frame.
    await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
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
    <DaemonProvider value={fakeDaemon()}>
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
    action = useDialogAction();
    return null;
  }
  try {
    act(() => root.render(<Subject />));
    await act(() => action.run(() => Promise.reject(new Error("Something failed"))));
    expect(errorAlerts.getSnapshot().some((error) => error.message === "Something failed")).toBe(true);
    expect(action.busy).toBe(false);
    expect(container.querySelector('[role="alert"]')).toBeNull();
  } finally {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
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
