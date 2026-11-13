// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { sessionOf } from "../fixtures/builders";
import type { ExitCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { SessionStatus, ShellTerminal } from "../protocol";
import { HANDOVER_CHECK_TIMEOUT_MS, quitHolders, useAppExit, type HandoverCheck } from "./useAppExit";
import { BROWSER_HOST } from "../test/hosts";
import { fakeTimers } from "../test/clock";

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
    ["dormant", true],
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
  const shell = (id: string, busy: boolean): ShellTerminal => ({ id, project_id: "p", worktree: null, cwd: "/p", title: "zsh", renamed: false, state: "running", busy, created_at: 0 });
  /** What the daemon answers with a proven answer that lists `lists`. */
  const known = (lists: Partial<HandoverCheck> = {}): HandoverCheck => ({ known: true, sessions: [], shellTerminals: [], unreachedShellTerminals: [], ...lists });
  const unknown: HandoverCheck = { known: false, sessions: [], shellTerminals: [], unreachedShellTerminals: [] };

  function mount(
    sessions: Parameters<typeof quitHolders>[0],
    options: { confirmQuit?: ExitCapability["confirmQuit"]; handover?: HandoverCheck | Error | "never"; terminals?: ShellTerminal[]; withoutCheck?: boolean } = {},
  ) {
    const { confirmQuit = async () => {}, handover = known(), terminals = [], withoutCheck = false } = options;
    let quitGesture = () => {};
    const order: string[] = [];
    const exit: ExitCapability = {
      register: ({ onQuitRequested }) => {
        quitGesture = onQuitRequested;
        return () => {};
      },
      heartbeat: async () => {},
      beginQuit: vi.fn(async () => void order.push("beginQuit")),
      confirmQuit: vi.fn(async (...args: Parameters<ExitCapability["confirmQuit"]>) => {
        order.push("confirmQuit");
        return confirmQuit(...args);
      }),
    };
    const requestShutdown = vi.fn(async () => void order.push("shutdown"));
    const requestPlannedStop = vi.fn(async () => {});
    const checkHandover = vi.fn(async (_floor: string | null, _version: string | null): Promise<HandoverCheck> => {
      if (handover === "never") return new Promise(() => {});
      if (handover instanceof Error) throw handover;
      return handover;
    });
    let current!: ReturnType<typeof useAppExit>;
    const Host = () => {
      current = useAppExit({ getSessions: () => [...sessions], getShellTerminals: () => [...terminals], requestShutdown, requestPlannedStop, checkHandover: withoutCheck ? undefined : checkHandover });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "tauri", host: BROWSER_HOST, exit } }, createElement(Host))));
    return { exit, order, requestShutdown, requestPlannedStop, checkHandover, hook: () => current, quit: () => quitGesture() };
  }

  it("asks the daemon even when nothing is running, and relaunches at once on an answer that lists nothing", async () => {
    const { exit, requestShutdown, requestPlannedStop, checkHandover, hook } = mount([]);
    await act(async () => hook().requestRestart(null, null));
    expect(checkHandover).toHaveBeenCalledOnce();
    expect(requestPlannedStop).toHaveBeenCalledOnce();
    expect(requestShutdown).not.toHaveBeenCalled();
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("hands the update's holder floor and version to the daemon's check", async () => {
    const { checkHandover, hook } = mount([sessionOf("s", "c", "p", "S", "working")]);
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(checkHandover).toHaveBeenCalledExactlyOnceWith("0.2.0", "0.3.0");
  });

  it("relaunches without asking when a proven answer lists nothing, whatever is running", async () => {
    const { exit, requestPlannedStop, hook } = mount([sessionOf("s", "c", "p", "S", "working"), sessionOf("i", "c", "p", "I", "idle")], { terminals: [shell("t", true)] });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook().exitConfirmOpen).toBe(false);
    expect(requestPlannedStop).toHaveBeenCalledOnce();
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("asks for an idle session the daemon lists, which a quit would not ask for", async () => {
    const { exit, hook } = mount([sessionOf("i", "c", "p", "I", "idle")], { handover: known({ sessions: ["i"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitRestarts: true, exitHolders: { sessions: true, terminals: false }, exitKeepsRest: true });
    expect(hook().exitInterrupted.map((s) => s.id)).toEqual(["i"]);
    expect(hook().exitRestartDetail).toEqual({ uncertain: false, unnamedSessions: 0, unreachedTerminals: 0 });
    expect(exit.confirmQuit).not.toHaveBeenCalled();
  });

  it("names the idle incompatible session beside a compatible busy one the daemon does not list", async () => {
    const { hook } = mount([sessionOf("w", "c", "p", "W", "working"), sessionOf("i", "c", "p", "I", "idle")], { handover: known({ sessions: ["i"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook().exitConfirmOpen).toBe(true);
    expect(hook().exitInterrupted.map((s) => s.id)).toEqual(["i"]);
  });

  it("asks first, naming the sessions the daemon lists, and relaunches only once confirmed", async () => {
    const working = sessionOf("s", "c", "p", "S", "working");
    const { exit, requestPlannedStop, hook } = mount([working, sessionOf("o", "c", "p", "O", "working"), sessionOf("i", "c", "p", "I", "idle")], { handover: known({ sessions: ["s", "i"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitRestarts: true, exitHolders: { sessions: true, terminals: false } });
    expect(hook().exitInterrupted.map((s) => s.id)).toEqual(["s", "i"]);
    expect(hook().exitKeepsRest).toBe(true);
    expect(exit.confirmQuit).not.toHaveBeenCalled();
    expect(requestPlannedStop).not.toHaveBeenCalled();
    await act(async () => hook().confirmExit());
    expect(requestPlannedStop).toHaveBeenCalledOnce();
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
  });

  it("keeps the ids the daemon lists that this window does not know, as a count", async () => {
    const { hook } = mount([sessionOf("s", "c", "p", "S", "working")], { handover: known({ sessions: ["s", "gone-1", "gone-2"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook().exitInterrupted.map((s) => s.id)).toEqual(["s"]);
    expect(hook().exitRestartDetail.unnamedSessions).toBe(2);
    const onlyUnknown = mount([], { handover: known({ sessions: ["gone"] }) });
    await act(async () => onlyUnknown.hook().requestRestart("0.2.0", "0.3.0"));
    expect(onlyUnknown.hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: true, terminals: false }, exitInterrupted: [] });
    expect(onlyUnknown.hook().exitRestartDetail.unnamedSessions).toBe(1);
  });

  it("asks for a listed busy shell terminal, and for no other one", async () => {
    const terminals = [shell("busy", true), shell("other", true), shell("idle", false)];
    const asked = mount([], { handover: known({ shellTerminals: ["busy"] }), terminals });
    await act(async () => asked.hook().requestRestart("0.2.0", "0.3.0"));
    expect(asked.hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: false, terminals: true } });
    const idle = mount([sessionOf("w", "c", "p", "W", "working")], { handover: known({ shellTerminals: ["idle"] }), terminals });
    await act(async () => idle.hook().requestRestart("0.2.0", "0.3.0"));
    expect(idle.hook().exitConfirmOpen).toBe(false);
  });

  it("cannot say everything else keeps running when a listed terminal at its prompt would end unmentioned", async () => {
    const terminals = [shell("busy", true), shell("idle", false)];
    const { hook } = mount([], { handover: known({ shellTerminals: ["busy", "idle"] }), terminals });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitKeepsRest: false });
  });

  it("asks for terminals of an unreached holder, whose busy state is not known, and cannot say the rest keeps running", async () => {
    const { hook } = mount([], { handover: known({ unreachedShellTerminals: ["p1", "p2"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: false, terminals: false }, exitKeepsRest: false });
    expect(hook().exitRestartDetail.unreachedTerminals).toBe(2);
  });

  it("counts a listed terminal this window does not hold as a busy one, not as unreached", async () => {
    const { hook } = mount([], { handover: known({ shellTerminals: ["gone"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: false, terminals: true }, exitKeepsRest: true });
    expect(hook().exitRestartDetail.unreachedTerminals).toBe(0);
  });

  it("names an idle listed session beside a busy listed terminal", async () => {
    const { hook } = mount([sessionOf("i", "c", "p", "I", "idle")], { handover: known({ sessions: ["i"], shellTerminals: ["busy"] }), terminals: [shell("busy", true)] });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: true, terminals: true }, exitKeepsRest: true });
    expect(hook().exitInterrupted.map((s) => s.id)).toEqual(["i"]);
  });

  describe("without a proven answer", () => {
    const sessions = [
      sessionOf("w", "c", "p", "W", "working"),
      sessionOf("i", "c", "p", "I", "idle"),
      { ...sessionOf("p", "c", "p", "P", "idle"), holder_link: "pending" as const },
      sessionOf("x", "c", "p", "X", "interrupted"),
      sessionOf("a", "c", "p", "A", "archived"),
    ];

    it.each([
      ["failure", new Error("closed")],
      ["a answer that is not known", unknown],
    ] as const)("warns, with uncertain copy, for every session with a running process on %s", async (_name, handover) => {
      const { exit, hook } = mount(sessions, { handover });
      await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
      expect(hook()).toMatchObject({ exitConfirmOpen: true, exitRestarts: true, exitKeepsRest: false });
      expect(hook().exitInterrupted.map((s) => s.id)).toEqual(["w", "i", "p"]);
      expect(hook().exitRestartDetail).toEqual({ uncertain: true, unnamedSessions: 0, unreachedTerminals: 0 });
      expect(exit.confirmQuit).not.toHaveBeenCalled();
    });

    it("keeps the terminals the daemon could not reach from an answer that is not known, and asks for them alone", async () => {
      const { hook } = mount([], { handover: { ...unknown, unreachedShellTerminals: ["p1", "p2"] } });
      await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
      expect(hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: false, terminals: false }, exitKeepsRest: false });
      expect(hook().exitRestartDetail).toEqual({ uncertain: true, unnamedSessions: 0, unreachedTerminals: 2 });
    });

    it("warns for an idle session alone, and for a busy shell terminal alone", async () => {
      const idle = mount([sessionOf("i", "c", "p", "I", "idle")], { handover: unknown });
      await act(async () => idle.hook().requestRestart("0.2.0", "0.3.0"));
      expect(idle.hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: true, terminals: false } });
      const busy = mount([], { handover: unknown, terminals: [shell("b", true), shell("i", false)] });
      await act(async () => busy.hook().requestRestart("0.2.0", "0.3.0"));
      expect(busy.hook()).toMatchObject({ exitConfirmOpen: true, exitHolders: { sessions: false, terminals: true } });
    });

    it("warns when the daemon does not answer in time, and when there is no check to ask", async () => {
      fakeTimers();
      const slow = mount([sessionOf("i", "c", "p", "I", "idle")], { handover: "never" });
      const pending = act(async () => slow.hook().requestRestart("0.2.0", "0.3.0"));
      await act(async () => vi.advanceTimersByTimeAsync(HANDOVER_CHECK_TIMEOUT_MS));
      await pending;
      expect(slow.hook()).toMatchObject({ exitConfirmOpen: true, exitRestartDetail: { uncertain: true } });
      const none = mount([sessionOf("i", "c", "p", "I", "idle")], { withoutCheck: true });
      await act(async () => none.hook().requestRestart("0.2.0", "0.3.0"));
      expect(none.hook()).toMatchObject({ exitConfirmOpen: true, exitRestartDetail: { uncertain: true } });
    });

    it("relaunches without asking when nothing has a running process or is busy", async () => {
      const { exit, hook } = mount([sessionOf("x", "c", "p", "X", "interrupted")], { handover: new Error("closed"), terminals: [shell("t", false)] });
      await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
      expect(hook().exitConfirmOpen).toBe(false);
      expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
    });
  });

  it("tells the shell the quit has begun, then shuts the daemon down, without asking the handover check", async () => {
    const { exit, order, requestShutdown, requestPlannedStop, checkHandover, hook, quit } = mount([sessionOf("s", "c", "p", "S", "working")]);
    await act(async () => quit());
    expect(hook()).toMatchObject({ exitConfirmOpen: true, exitRestarts: false, exitInterrupted: [] });
    await act(async () => hook().confirmExit());
    expect(checkHandover).not.toHaveBeenCalled();
    expect(order).toEqual(["beginQuit", "shutdown", "confirmQuit"]);
    expect(requestShutdown).toHaveBeenCalledOnce();
    expect(requestPlannedStop).not.toHaveBeenCalled();
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: false });
  });

  it("does not tell the shell a quit has begun for a restart", async () => {
    const { exit, hook } = mount([]);
    await act(async () => hook().requestRestart(null, null));
    expect(exit.beginQuit).not.toHaveBeenCalled();
  });

  it("quits even when the shell cannot record the quit", async () => {
    const { exit, hook } = mount([]);
    vi.mocked(exit.beginQuit).mockRejectedValueOnce(new Error("no"));
    await act(async () => hook().requestQuit());
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: false });
  });

  it("leaves everything running when the user cancels the confirmation", async () => {
    const { exit, requestShutdown, requestPlannedStop, hook } = mount([sessionOf("s", "c", "p", "S", "working")], { handover: known({ sessions: ["s"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    act(() => hook().closeExitConfirm());
    expect(hook().exitConfirmOpen).toBe(false);
    expect(requestShutdown).not.toHaveBeenCalled();
    expect(requestPlannedStop).not.toHaveBeenCalled();
    expect(exit.confirmQuit).not.toHaveBeenCalled();
  });

  it("turns an open restart confirmation into a quit when a quit is asked for meanwhile", async () => {
    const { exit, hook, quit } = mount([sessionOf("s", "c", "p", "S", "working")], { handover: known({ sessions: ["s"] }) });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    await act(async () => quit());
    expect(hook().exitRestarts).toBe(false);
    expect(hook().exitInterrupted).toEqual([]);
    await act(async () => hook().confirmExit());
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: false });
  });

  it.each([
    ["restart", "Could not restart", "Octoboard kept running instead of relaunching. Quit it and open it again."],
    ["quit", "Could not quit", "Octoboard kept running instead of ending. Try quitting it again."],
  ] as const)("says in its own words that a %s did not happen, with the shell's text behind Details", async (action, title, message) => {
    const { hook, quit } = mount([], { confirmQuit: async () => { throw new Error("ipc: no such window"); } });
    await act(async () => (action === "restart" ? hook().requestRestart(null, null) : quit()));
    const [alert] = errorAlerts.getSnapshot();
    onTestFinished(() => act(() => errorAlerts.acknowledge()));
    expect(alert.title).toBe(title);
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
      beginQuit: async () => {},
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
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "tauri", host: BROWSER_HOST, exit } }, createElement(Host))));
    act(() => exited("signal 9"));
    const [alert] = errorAlerts.getSnapshot();
    onTestFinished(() => act(() => errorAlerts.acknowledge()));
    expect(alert.title).toBe("The daemon exited unexpectedly");
    expect(alert.message).toBe("The daemon runs your sessions and terminals, and Octoboard cannot work without it. Restart Octoboard to continue.");
    expect(alert.message).not.toContain("signal");
    expect(alert.details).not.toBeNull();
    expect(alert.classification.kind).toBe("unexpected");
  });
});

describe("restarting the daemon a browser tab is served by", () => {
  function mount({ restartDaemon, handover }: { restartDaemon: (target: string | null) => Promise<unknown>; handover: HandoverCheck }) {
    const checkHandover = vi.fn(async () => handover);
    let current!: ReturnType<typeof useAppExit>;
    const Host = () => {
      current = useAppExit({ getSessions: () => [sessionOf("s", "c", "p", "S", "working")], restartDaemon, checkHandover });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "browser", host: BROWSER_HOST } }, createElement(Host))));
    return { checkHandover, hook: () => current };
  }
  const proven: HandoverCheck = { known: true, sessions: [], shellTerminals: [], unreachedShellTerminals: [] };

  it("sends the restart, with the version it goes to, once the daemon says nothing is interrupted", async () => {
    const restartDaemon = vi.fn(async () => {});
    const { checkHandover, hook } = mount({ restartDaemon, handover: proven });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(checkHandover).toHaveBeenCalledExactlyOnceWith("0.2.0", "0.3.0");
    expect(restartDaemon).toHaveBeenCalledExactlyOnceWith("0.3.0");
  });

  it("asks first when sessions would be interrupted, and sends the restart only once confirmed", async () => {
    const restartDaemon = vi.fn(async () => {});
    const { hook } = mount({ restartDaemon, handover: { ...proven, sessions: ["s"] } });
    await act(async () => hook().requestRestart("0.2.0", "0.3.0"));
    expect(hook().exitConfirmOpen).toBe(true);
    expect(restartDaemon).not.toHaveBeenCalled();
    await act(async () => hook().confirmExit());
    expect(restartDaemon).toHaveBeenCalledExactlyOnceWith("0.3.0");
    expect(hook().exitConfirmOpen).toBe(false);
  });

  it("does not wait for an answer the exiting daemon never gives, and reports a refusal in the restart's words", async () => {
    fakeTimers();
    const hung = mount({ restartDaemon: () => new Promise(() => {}), handover: proven });
    const request = act(async () => hung.hook().requestRestart(null, "0.3.0"));
    await act(async () => vi.advanceTimersByTimeAsync(2500));
    await request;
    expect(errorAlerts.getSnapshot()).toHaveLength(0);

    const refused = mount({ restartDaemon: async () => { throw new CodedError("Shutting down.", "daemon_shutting_down", {}); }, handover: proven });
    await act(async () => refused.hook().requestRestart(null, "0.3.0"));
    onTestFinished(() => act(() => errorAlerts.acknowledge()));
    expect(errorAlerts.getSnapshot()).toHaveLength(1);
    expect(errorAlerts.getSnapshot()[0]?.title).toBe("Could not restart");
    expect(errorAlerts.getSnapshot()[0]?.classification.kind).toBe("actionable");
  });

  it("never shuts the daemon down for a tab's quit, with the restart option or without", async () => {
    const restartDaemon = vi.fn(async () => {});
    const { hook } = mount({ restartDaemon, handover: proven });
    await act(async () => hook().requestQuit());
    expect(hook().exitConfirmOpen).toBe(false);
    expect(restartDaemon).not.toHaveBeenCalled();
  });

  it("leaves the restart to the shell where there is one", async () => {
    const restartDaemon = vi.fn(async () => {});
    const exit: ExitCapability = { register: () => () => {}, heartbeat: async () => {}, beginQuit: vi.fn(async () => {}), confirmQuit: vi.fn(async () => {}) };
    const requestPlannedStop = vi.fn(async () => {});
    let current!: ReturnType<typeof useAppExit>;
    const Host = () => {
      current = useAppExit({ getSessions: () => [], requestShutdown: async () => {}, requestPlannedStop, restartDaemon, checkHandover: async () => ({ known: true, sessions: [], shellTerminals: [], unreachedShellTerminals: [] }) });
      return null;
    };
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "tauri", host: BROWSER_HOST, exit } }, createElement(Host))));
    await act(async () => current.requestRestart(null, "0.3.0"));
    expect(requestPlannedStop).toHaveBeenCalledOnce();
    expect(exit.confirmQuit).toHaveBeenCalledExactlyOnceWith({ restart: true });
    expect(restartDaemon).not.toHaveBeenCalled();
  });

  it("takes a request cut short by the exiting daemon, or timed out, as the restart under way, and says nothing", async () => {
    for (const code of ["connection_closed", "request_timed_out"]) {
      const { hook } = mount({ restartDaemon: async () => { throw new CodedError("gone", code, {}); }, handover: proven });
      await act(async () => hook().requestRestart(null, "0.3.0"));
      expect(errorAlerts.getSnapshot()).toHaveLength(0);
    }
  });

  it("asks for nothing a tab can do without that option", async () => {
    const container = document.body.appendChild(document.createElement("div"));
    const root = createRoot(container);
    onTestFinished(() => {
      act(() => root.unmount());
      container.remove();
    });
    let current!: ReturnType<typeof useAppExit>;
    const Host = () => {
      current = useAppExit({ getSessions: () => [] });
      return null;
    };
    act(() => root.render(createElement(PlatformProvider, { value: { kind: "browser", host: BROWSER_HOST } }, createElement(Host))));
    await act(async () => current.requestRestart(null, null));
    expect(current.exitConfirmOpen).toBe(false);
  });
});
