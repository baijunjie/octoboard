// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { sessionOf } from "../gallery/fixtures/builders";
import type { ExitCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { SessionStatus, ShellTerminal } from "../protocol";
import { quitHolders, useAppExit } from "./useAppExit";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Whether anything holds the quit up. */
const holdsQuit = (...args: Parameters<typeof quitHolders>) => {
  const holders = quitHolders(...args);
  return holders.sessions || holders.terminals;
};

describe("quitHolders", () => {
  it.each<[SessionStatus, boolean]>([
    ["starting", true],
    ["working", true],
    ["waiting_user", true],
    ["idle", false],
    ["interrupted", false],
    ["archived", false],
  ])("a session %s holds the quit up: %s", (status, expected) => {
    expect(holdsQuit([sessionOf("s", "c", "p", "S", status)])).toBe(expected);
  });

  it("asks for a session awaiting instructions only while it has work running in the background", () => {
    const idle = sessionOf("s", "c", "p", "S", "idle");
    expect(holdsQuit([{ ...idle, background_tasks: [{ id: "b1" }] }])).toBe(true);
    expect(holdsQuit([{ ...idle, background_tasks: [] }])).toBe(false);
  });

  it("asks when any one session holds it up", () => {
    expect(holdsQuit([sessionOf("a", "c", "p", "A", "idle"), sessionOf("b", "c", "p", "B", "working")])).toBe(true);
  });

  it("does not ask with no sessions", () => {
    expect(holdsQuit([])).toBe(false);
  });

  it("asks while a shell terminal runs something, never for an idle or exited one", () => {
    const shell = (state: ShellTerminal["state"], busy: boolean) => ({ id: "t", project_id: "p", worktree: null, cwd: "/p", title: "zsh", renamed: false, state, busy, created_at: 0 });
    expect(holdsQuit([], [shell("running", true)])).toBe(true);
    expect(holdsQuit([], [shell("running", false)])).toBe(false);
    expect(holdsQuit([], [shell("exited", false)])).toBe(false);
    expect(quitHolders([sessionOf("a", "c", "p", "A", "working")], [shell("running", true)])).toEqual({ sessions: true, terminals: true });
  });
});

describe("restarting", () => {
  function mount(sessions: Parameters<typeof quitHolders>[0], confirmQuit: ExitCapability["confirmQuit"] = async () => {}) {
    const exit: ExitCapability = {
      register: () => () => {},
      heartbeat: async () => {},
      confirmQuit: vi.fn(confirmQuit),
    };
    const requestShutdown = vi.fn(async () => {});
    let current!: ReturnType<typeof useAppExit>;
    const Host = () => {
      current = useAppExit({ getSessions: () => [...sessions], requestShutdown });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "tauri", exit } }, createElement(Host))));
    return { exit, requestShutdown, hook: () => current };
  }

  it("relaunches at once when nothing is running, after shutting the daemon down", async () => {
    const { exit, requestShutdown, hook } = mount([]);
    await act(async () => hook().requestRestart());
    expect(requestShutdown).toHaveBeenCalledOnce();
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("asks first when a session is running, and relaunches only once confirmed", async () => {
    const { exit, hook } = mount([sessionOf("s", "c", "p", "S", "working")]);
    await act(async () => hook().requestRestart());
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitRestarts: true });
    expect(exit.confirmQuit).not.toHaveBeenCalled();
    await act(async () => hook().confirmExit());
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("leaves everything running when the user cancels the confirmation", async () => {
    const { exit, requestShutdown, hook } = mount([sessionOf("s", "c", "p", "S", "working")]);
    await act(async () => hook().requestRestart());
    act(() => hook().closeExitConfirm());
    expect(hook().exitConfirmOpen).toBe(false);
    expect(requestShutdown).not.toHaveBeenCalled();
    expect(exit.confirmQuit).not.toHaveBeenCalled();
  });

  it("turns an open restart confirmation into a quit when a quit is asked for meanwhile", async () => {
    const { exit, hook } = mount([sessionOf("s", "c", "p", "S", "working")]);
    await act(async () => hook().requestRestart());
    await act(async () => hook().requestQuit());
    expect(hook().exitRestarts).toBe(false);
    await act(async () => hook().confirmExit());
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: false });
  });

  it.each([
    ["restart", "Could not restart Octoboard."],
    ["quit", "Could not quit Octoboard."],
  ] as const)("says in its own words that a %s did not happen, with the shell's text behind Details", async (action, message) => {
    const { hook } = mount([], async () => { throw new Error("ipc: no such window"); });
    await act(async () => (action === "restart" ? hook().requestRestart() : hook().requestQuit()));
    const [alert] = errorAlerts.getSnapshot();
    onTestFinished(() => act(() => errorAlerts.acknowledge()));
    expect(alert.message).toBe(message);
    expect(alert.message).not.toContain("ipc");
    expect(alert.details).not.toBeNull();
    // Nothing the user did explains it, so the alert offers Report issue.
    expect(alert.classification.kind).toBe("unexpected");
  });
});

describe("the daemon exiting by itself", () => {
  it("says so in the application's words, with the shell's text behind Details", async () => {
    let exited: (detail: string) => void = () => {};
    const exit: ExitCapability = {
      register: ({ onDaemonExited }) => {
        exited = onDaemonExited;
        return () => {};
      },
      heartbeat: async () => {},
      confirmQuit: async () => {},
    };
    const Host = () => {
      useAppExit();
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "tauri", exit } }, createElement(Host))));
    act(() => exited("signal 9"));
    const [alert] = errorAlerts.getSnapshot();
    onTestFinished(() => act(() => errorAlerts.acknowledge()));
    expect(alert.message).toBe("The daemon process exited unexpectedly. Restart Octoboard to continue.");
    expect(alert.message).not.toContain("signal");
    expect(alert.details).not.toBeNull();
    expect(alert.classification.kind).toBe("unexpected");
  });
});
