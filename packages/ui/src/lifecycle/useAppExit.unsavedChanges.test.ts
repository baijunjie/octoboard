// @vitest-environment jsdom
// How a quit, a restart and a browser tab closed or reloaded treat changes a page form holds that are not saved:
// asked about first (before the question about what is running), a quit that goes on only once they are given up, and
// the browser's own warning. The rest of `useAppExit` is in `useAppExit.test.ts`.
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { createUnsavedChangesGuard, type UnsavedChangesGuard } from "../components/dialog/unsavedChangesGuard";
import { sessionOf } from "../fixtures/builders";
import type { ExitCapability, NativeWindowCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import { flushPromises } from "../test/clock";
import { BROWSER_HOST } from "../test/hosts";
import { useAppExit } from "./useAppExit";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const reloading = vi.hoisted(() => ({ now: false }));
vi.mock("../platform/reloadPage", () => ({ isReloading: () => reloading.now, reloadPage: () => {} }));

/** Answers the question Discard, and lets what the gesture goes on with run to its end. */
async function discardAnswered(guard: UnsavedChangesGuard): Promise<void> {
  await act(async () => void guard.answer(true)?.());
  await flushPromises();
}

/** A guard holding one form with changes not saved, whose Discard is `discard`. */
function dirtyGuard(discard: () => void = () => {}): UnsavedChangesGuard {
  const guard = createUnsavedChangesGuard();
  guard.hold({}, { saving: false, discard });
  return guard;
}

describe("a quit or a restart", () => {
  function mount(unsaved: UnsavedChangesGuard | undefined, { running = true }: { running?: boolean } = {}) {
    const order: string[] = [];
    let quitGesture = () => {};
    const exit: ExitCapability = {
      register: ({ onQuitRequested }) => {
        quitGesture = onQuitRequested;
        return () => {};
      },
      heartbeat: vi.fn(async () => void order.push("heartbeat")),
      beginQuit: vi.fn(async () => void order.push("beginQuit")),
      confirmQuit: vi.fn(async () => void order.push("confirmQuit")),
    };
    const nativeWindow = { bringToFront: vi.fn(async () => void order.push("bringToFront")), isVisible: async () => true } as unknown as NativeWindowCapability;
    const sessions = running ? [sessionOf("s", "c", "p", "S", "working")] : [];
    const requestShutdown = vi.fn(async () => void order.push("shutdown"));
    let current!: ReturnType<typeof useAppExit>;
    const Host = () => {
      current = useAppExit({ unsaved, getSessions: () => sessions, getShellTerminals: () => [], requestShutdown, requestPlannedStop: async () => {} });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "tauri", host: BROWSER_HOST, exit, nativeWindow } }, createElement(Host))));
    return { exit, order, hook: () => current, quit: () => quitGesture(), requestShutdown };
  }

  it("asks about the changes first, after the heartbeat and before the running sessions, with the window brought forward", async () => {
    const guard = dirtyGuard();
    const { order, hook, quit } = mount(guard);
    await act(async () => quit());
    expect(guard.getSnapshot().asking).toBe(true);
    expect(order).toEqual(["heartbeat", "bringToFront"]);
    // What is running is not asked about until the changes are given up.
    expect(hook().exitConfirmOpen).toBe(false);
  });

  it("goes on into the usual quit flow once Discard is answered, which asks about what is running", async () => {
    const discard = vi.fn();
    const guard = dirtyGuard(discard);
    const { exit, order, requestShutdown, hook, quit } = mount(guard);
    await act(async () => quit());
    await discardAnswered(guard);
    expect(discard).toHaveBeenCalledTimes(1);
    expect(guard.getSnapshot()).toEqual({ dirty: false, asking: false });
    expect(hook().exitConfirmOpen).toBe(true);
    expect(exit.confirmQuit).not.toHaveBeenCalled();
    await act(async () => hook().confirmExit());
    expect(order).toEqual(["heartbeat", "bringToFront", "bringToFront", "beginQuit", "shutdown", "confirmQuit"]);
    expect(requestShutdown).toHaveBeenCalledOnce();
  });

  it("quits at once after Discard where nothing is running", async () => {
    const guard = dirtyGuard();
    const { exit, quit } = mount(guard, { running: false });
    await act(async () => quit());
    expect(exit.confirmQuit).not.toHaveBeenCalled();
    await discardAnswered(guard);
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: false });
  });

  it("stops at Keep editing: nothing is asked about what is running, nothing is begun, and the changes stay held", async () => {
    const discard = vi.fn();
    const guard = dirtyGuard(discard);
    const { exit, hook, quit } = mount(guard);
    await act(async () => quit());
    await act(async () => void guard.answer(false));
    expect(guard.getSnapshot()).toEqual({ dirty: true, asking: false });
    expect(discard).not.toHaveBeenCalled();
    expect(hook().exitConfirmOpen).toBe(false);
    expect(exit.beginQuit).not.toHaveBeenCalled();
    expect(exit.confirmQuit).not.toHaveBeenCalled();
  });

  it("asks for a restart for an update the same way, before the daemon is asked what it would interrupt", async () => {
    const guard = dirtyGuard();
    const checkHandover = vi.fn(async () => ({ known: true as const, sessions: [], shellTerminals: [], unreachedShellTerminals: [] }));
    const exit: ExitCapability = { register: () => () => {}, heartbeat: async () => {}, beginQuit: async () => {}, confirmQuit: vi.fn(async () => {}) };
    let current!: ReturnType<typeof useAppExit>;
    const Host = () => {
      current = useAppExit({ unsaved: guard, getSessions: () => [], getShellTerminals: () => [], requestShutdown: async () => {}, requestPlannedStop: async () => {}, checkHandover });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "tauri", host: BROWSER_HOST, exit } }, createElement(Host))));
    await act(async () => current.requestRestart("0.2.0", "0.3.0"));
    expect(guard.getSnapshot().asking).toBe(true);
    expect(checkHandover).not.toHaveBeenCalled();
    await discardAnswered(guard);
    expect(checkHandover).toHaveBeenCalledOnce();
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("asks for the restart a window makes at once (a daemon that is gone) as well, and restarts only once Discard is answered", async () => {
    const guard = dirtyGuard();
    const { exit, order, hook } = mount(guard);
    let underWay: boolean | undefined;
    await act(async () => {
      underWay = await hook().restartNow();
    });
    expect(underWay).toBe(false);
    expect(guard.getSnapshot().asking).toBe(true);
    expect(order).toEqual(["bringToFront"]);
    expect(exit.confirmQuit).not.toHaveBeenCalled();
    await discardAnswered(guard);
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("does not restart at once when the answer is Keep editing", async () => {
    const guard = dirtyGuard();
    const { exit, hook } = mount(guard);
    await act(async () => void hook().restartNow());
    await act(async () => void guard.answer(false));
    expect(exit.confirmQuit).not.toHaveBeenCalled();
    expect(guard.getSnapshot()).toEqual({ dirty: true, asking: false });
  });

  it("restarts at once, asking nothing, where no changes are held", async () => {
    const guard = createUnsavedChangesGuard();
    const { exit, hook } = mount(guard);
    let underWay: boolean | undefined;
    await act(async () => {
      underWay = await hook().restartNow();
    });
    expect(underWay).toBe(true);
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("asks nothing about changes where none are held, and goes straight to what is running", async () => {
    const guard = createUnsavedChangesGuard();
    const { order, hook, quit } = mount(guard);
    await act(async () => quit());
    expect(guard.getSnapshot().asking).toBe(false);
    expect(hook().exitConfirmOpen).toBe(true);
    expect(order).toEqual(["heartbeat", "bringToFront"]);
  });
});

describe("a browser tab closed or reloaded", () => {
  function mountInBrowser(unsaved: UnsavedChangesGuard | undefined, kind: "browser" | "tauri" = "browser") {
    const Host = () => {
      useAppExit({ unsaved });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind, host: BROWSER_HOST } }, createElement(Host))));
  }
  /** Whether the browser is asked to warn: the event is cancelled by a page that wants the warning. */
  const warns = () => {
    const event = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(event);
    return event.defaultPrevented;
  };

  it("gets the browser's own warning while a form holds changes that are not saved, and none once they are gone", () => {
    const guard = createUnsavedChangesGuard();
    mountInBrowser(guard);
    expect(warns()).toBe(false);
    const key = {};
    guard.hold(key, { saving: false, discard: () => {} });
    expect(warns()).toBe(true);
    guard.release(key);
    expect(warns()).toBe(false);
  });

  it("is not warned about while the page reloads itself", () => {
    const guard = dirtyGuard();
    mountInBrowser(guard);
    expect(warns()).toBe(true);
    reloading.now = true;
    onTestFinished(() => {
      reloading.now = false;
    });
    expect(warns()).toBe(false);
  });

  it("is not warned about where the window is the desktop shell's, which asks through the quit instead", () => {
    mountInBrowser(dirtyGuard(), "tauri");
    expect(warns()).toBe(false);
  });

  it("stops warning once the window is unmounted", () => {
    const guard = dirtyGuard();
    const Host = () => {
      useAppExit({ unsaved: guard });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "browser", host: BROWSER_HOST } }, createElement(Host))));
    expect(warns()).toBe(true);
    act(() => root.unmount());
    container.remove();
    expect(warns()).toBe(false);
  });
});
