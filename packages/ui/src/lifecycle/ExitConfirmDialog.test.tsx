// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, onTestFinished, vi } from "vitest";

import { LanguageProvider } from "../i18n/react";
import { sessionOf } from "../fixtures/builders";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { exitMessage, ExitConfirmDialog, MAX_NAMED_INTERRUPTED } from "./ExitConfirmDialog";
import type { UseAppExitResult } from "./useAppExit";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const NOTHING = { sessions: false, terminals: false };

/** The open restart confirmation `useAppExit` reports for `titles`, unless `change` says otherwise. */
function restartOf(titles: string[], change: Partial<UseAppExitResult> = {}): UseAppExitResult {
  return {
    exitConfirmOpen: true,
    exitHolders: { sessions: true, terminals: false },
    exitInterrupted: titles.map((title, i) => sessionOf(`s${i}`, "c", "p", title, "working")),
    exitKeepsRest: true,
    exitRestartDetail: DETAIL,
    exitRestarts: true,
    closeExitConfirm: () => {},
    confirmExit: async () => {},
    requestRestart: async () => {},
    requestQuit: async () => {},
    ...change,
  };
}

function mount(exit: UseAppExitResult): HTMLElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => {
    root.render(
      <DaemonProvider value={testDaemon()}>
        <LanguageProvider>
          <ExitConfirmDialog exit={exit} />
        </LanguageProvider>
      </DaemonProvider>,
    );
  });
  return container;
}

/** The dialog's text, which is rendered into the document. */
const shown = () => document.body.textContent ?? "";
const rows = () => [...document.querySelectorAll("[role=alertdialog] ul > li, [role=dialog] ul > li")];

const DETAIL = { uncertain: false, unnamedSessions: 0, unreachedTerminals: 0 };

describe("exitMessage", () => {
  it.each([
    [true, { sessions: false, terminals: true }, 1, "app.restart.interrupts.both"],
    [true, { sessions: false, terminals: true }, 0, "app.restart.message.terminals"],
    [true, { sessions: true, terminals: false }, 1, "app.restart.interrupts.sessions"],
    [true, { sessions: true, terminals: true }, 1, "app.restart.interrupts.both"],
    [false, { sessions: true, terminals: false }, 0, "app.quit.message"],
    [false, { sessions: false, terminals: true }, 0, "app.quit.message.terminals"],
    [false, { sessions: true, terminals: true }, 0, "app.quit.message.both"],
  ] as const)("restart %s, holders %j, %s named: %s", (restarts, holders, named, key) => {
    expect(exitMessage(restarts, holders, named, DETAIL)).toEqual({ key, unreachedInMessage: false });
  });

  it.each([
    [{ sessions: true, terminals: false }, 1, 0, "app.restart.uncertain.sessions"],
    [{ sessions: true, terminals: true }, 1, 0, "app.restart.uncertain.both"],
    [{ sessions: false, terminals: true }, 0, 0, "app.restart.uncertain.terminals"],
    [{ sessions: false, terminals: true }, 0, 2, "app.restart.uncertain.terminals"],
  ] as const)("an uncertain restart, holders %j, %s named, %s unreached: %s", (holders, named, unreachedTerminals, key) => {
    expect(exitMessage(true, holders, named, { ...DETAIL, uncertain: true, unreachedTerminals })).toEqual({ key, unreachedInMessage: false });
  });

  it("speaks of unreachable terminals alone only when nothing else asks, and says so", () => {
    expect(exitMessage(true, NOTHING, 0, { ...DETAIL, unreachedTerminals: 1 })).toEqual({ key: "app.restart.message.unreached", unreachedInMessage: true });
    expect(exitMessage(true, NOTHING, 0, { ...DETAIL, uncertain: true, unreachedTerminals: 1 })).toEqual({ key: "app.restart.message.unreached", unreachedInMessage: true });
    expect(exitMessage(true, { sessions: false, terminals: true }, 0, { ...DETAIL, unreachedTerminals: 1 })).toEqual({ key: "app.restart.message.terminals", unreachedInMessage: false });
  });

  it("states the count itself when no session can be named", () => {
    expect(exitMessage(true, { sessions: true, terminals: false }, 0, { ...DETAIL, unnamedSessions: 3 })).toEqual({ key: "app.restart.interrupts.counted", count: 3, unreachedInMessage: false });
    expect(exitMessage(true, { sessions: true, terminals: true }, 0, { ...DETAIL, unnamedSessions: 3 })).toEqual({ key: "app.restart.interrupts.counted.both", count: 3, unreachedInMessage: false });
    expect(exitMessage(true, { sessions: true, terminals: false }, 1, { ...DETAIL, unnamedSessions: 3 })).toMatchObject({ key: "app.restart.interrupts.sessions" });
  });
});

describe("ExitConfirmDialog", () => {
  it("renders nothing while closed", () => {
    mount(restartOf(["a"], { exitConfirmOpen: false }));
    expect(shown()).toBe("");
  });

  it("speaks of sessions and terminals for an idle listed session beside a busy listed terminal", () => {
    mount(restartOf(["Idle one"], { exitHolders: { sessions: false, terminals: true } }));
    expect(shown()).toContain("interrupts the sessions below and ends programs running in terminals");
    expect(shown()).toContain("Idle one");
  });

  it("names five sessions in full", () => {
    const titles = Array.from({ length: MAX_NAMED_INTERRUPTED }, (_, i) => `Session ${i + 1}`);
    mount(restartOf(titles));
    expect(rows()).toHaveLength(MAX_NAMED_INTERRUPTED);
    expect(shown()).not.toContain("more");
  });

  it("names four of six and ends the list with a row saying how many more", () => {
    const titles = Array.from({ length: MAX_NAMED_INTERRUPTED + 1 }, (_, i) => `Session ${i + 1}`);
    mount(restartOf(titles));
    expect(rows().map((row) => row.textContent)).toEqual([
      expect.stringContaining("Session 1"),
      expect.stringContaining("Session 2"),
      expect.stringContaining("Session 3"),
      expect.stringContaining("Session 4"),
      "and 2 more",
    ]);
    expect(shown()).not.toContain("Session 5");
  });

  it("says everything else keeps running only where it can", () => {
    mount(restartOf(["a"]));
    expect(shown()).toContain("Everything else keeps running.");
  });

  it("leaves that line out when it cannot be said", () => {
    mount(restartOf(["a"], { exitKeepsRest: false }));
    expect(shown()).not.toContain("Everything else keeps running.");
  });

  it("gathers the sessions it cannot name into the row saying how many more", () => {
    mount(restartOf(["Named one", "Named two"], { exitRestartDetail: { ...DETAIL, unnamedSessions: 2 } }));
    expect(rows().map((row) => row.textContent)).toEqual([expect.stringContaining("Named one"), expect.stringContaining("Named two"), "and 2 more"]);
  });

  it("gathers more than five, named or not, into one row", () => {
    const titles = Array.from({ length: MAX_NAMED_INTERRUPTED }, (_, i) => `Session ${i + 1}`);
    mount(restartOf(titles, { exitRestartDetail: { ...DETAIL, unnamedSessions: 3 } }));
    expect(rows()).toHaveLength(MAX_NAMED_INTERRUPTED);
    expect(rows().at(-1)?.textContent).toBe("and 4 more");
  });

  it("states how many sessions are interrupted when none can be named", () => {
    mount(restartOf([], { exitRestartDetail: { ...DETAIL, unnamedSessions: 3 } }));
    expect(shown()).toContain("Restarting interrupts 3 sessions. You can resume each one after the restart.");
    expect(rows()).toHaveLength(0);
    expect(shown()).toContain("Everything else keeps running.");
  });

  it("states the count with the terminals when programs in terminals end too", () => {
    mount(restartOf([], { exitHolders: { sessions: true, terminals: true }, exitRestartDetail: { ...DETAIL, unnamedSessions: 1 } }));
    expect(shown()).toContain("Restarting interrupts 1 session and ends programs running in terminals.");
  });

  it("says terminals that cannot be reached may end too, and not that the rest keeps running", () => {
    mount(restartOf(["a"], { exitKeepsRest: false, exitRestartDetail: { ...DETAIL, unreachedTerminals: 2 } }));
    expect(shown()).toContain("Terminals that cannot be reached right now may end too.");
    expect(shown()).not.toContain("Everything else keeps running.");
  });

  it("says so once when unreachable terminals are all there is", () => {
    mount(restartOf([], { exitHolders: NOTHING, exitKeepsRest: false, exitRestartDetail: { ...DETAIL, unreachedTerminals: 1 } }));
    expect(shown()).toContain("Some terminals cannot be reached right now. Restarting may end what they run.");
    expect(shown()).not.toContain("may end too");
  });

  it("words an uncertain restart as what may be interrupted, and never says the rest keeps running", () => {
    mount(restartOf(["Idle one"], { exitKeepsRest: false, exitRestartDetail: { ...DETAIL, uncertain: true } }));
    expect(shown()).toContain("Could not check which sessions keep running through the update. The sessions below may be interrupted.");
    expect(shown()).toContain("Idle one");
    expect(shown()).not.toContain("Everything else keeps running.");
  });

  it("asks about a quit with its own text and no list", () => {
    mount(restartOf([], { exitRestarts: false, exitHolders: { sessions: true, terminals: false }, exitKeepsRest: false }));
    expect(shown()).toContain("Quitting interrupts the running sessions.");
    expect(rows()).toHaveLength(0);
  });

  it("confirms and cancels through the flow", async () => {
    const confirmExit = vi.fn(async () => {});
    const closeExitConfirm = vi.fn();
    mount(restartOf(["a"], { confirmExit, closeExitConfirm, exitHolders: NOTHING }));
    const buttons = [...document.querySelectorAll("button")];
    await act(async () => buttons.find((b) => b.textContent === "Restart")!.click());
    expect(confirmExit).toHaveBeenCalledOnce();
    act(() => buttons.find((b) => b.textContent === "Cancel")!.click());
    expect(closeExitConfirm).toHaveBeenCalledOnce();
  });
});
