// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../fixtures/builders";
import { joinPhrases } from "../i18n/joinPhrases";
import { isolate } from "../i18n/isolate";
import { PlatformProvider } from "../platform/react";
import type { Session, SessionStatus } from "../protocol";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { Rail } from "./Rail";
import { enter, leave, mountForTooltip, openTooltip } from "../components/hint/tooltipTestSupport";
import { BROWSER_HOST } from "../test/hosts";
import { fakeTimers, runOutTimers } from "../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-1", "Main");

/** A session status beyond the wire's: the ones derived from an idle session (`delegated` adds a
 * working session bound to it), and `switching` account over any status. */
type Case = SessionStatus | "background" | "paused" | "done" | "delegated" | "switching" | "reconnecting";

const EXTRAS: Partial<Record<Case, Partial<Session>>> = {
  background: { background_tasks: [{ id: "watcher" }] },
  paused: { paused: true },
  done: { has_conversation: true },
  switching: { switching_account: true },
  reconnecting: { holder_link: "pending" },
};

/** Mounts a rail with one console, holding a session in each of `statuses`, whose avatar's pointer
 * hover floats the hidden sidebar in when `hoverOpensSidebar`. Returns the console's avatar button. */
function avatarOf(statuses: Case[], hoverOpensSidebar = false, otherSessions: Session[] = []): HTMLButtonElement {
  const wire = (status: Case): SessionStatus => (status === "background" || status === "paused" || status === "done" || status === "delegated" || status === "switching" || status === "reconnecting" ? "idle" : status);
  const sessions = statuses.flatMap((status, index) => {
    const own = sessionOf(`s-${index}`, main.id, undefined, `Hub ${index}`, wire(status), { has_conversation: false, ...EXTRAS[status] });
    return status === "delegated" ? [own, sessionOf(`s-${index}-child`, main.id, undefined, `Child ${index}`, "working", { bound_to: own.id })] : [own];
  });
  const daemon = testDaemon({ state: { sessions: new Map([...sessions, ...otherSessions].map((session) => [session.id, session])) } });
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
      <DaemonProvider value={daemon}>
        <Rail
          consoles={[main]}
          currentConsoleId={main.id}
          onSelectConsole={() => {}}
          onConsoleEnter={() => {}}
          onConsoleLeave={() => {}}
          hoverOpensSidebar={hoverOpensSidebar}
          onOpenDialog={() => {}}
          onSelectSession={() => {}}
          onOpenArchive={() => {}}
          waitingCount={0}
          onNextWaiting={() => {}}
          onOpenProcesses={() => true}
          onOpenSettings={() => {}}
          onOpenSettingsAt={() => {}}
          focusTerminal={() => {}}
          onDemand={false}
          away={false}
        />
      </DaemonProvider>
    </PlatformProvider>,
  );
  return container.querySelector<HTMLButtonElement>("[data-sidebar-peek-keep]")!;
}

function tooltipRows(): string[] {
  return [...openTooltip()!.querySelectorAll("dt")].map((term) => `${term.textContent} ${term.nextElementSibling!.textContent}`);
}

const COUNTS: Array<[Case[], string[]]> = [
  [["working", "waiting_user", "working"], ["Waiting for you 1", "Working 2"]],
  [["dormant", "paused", "done"], ["Dormant 1", "Paused 1", "Done 1"]],
  [["starting", "idle"], ["Awaiting instructions 1", "Starting 1"]],
  [["done", "paused", "waiting_user", "idle"], ["Waiting for you 1", "Paused 1", "Done 1", "Awaiting instructions 1"]],
  [["idle", "done", "done", "archived"], ["Done 2", "Awaiting instructions 1"]],
  [["background"], ["Working in background 1"]],
  [["delegated"], ["Child sessions working 1", "Working 1"]],
  [["switching"], ["Switching account 1"]],
  [["reconnecting", "done"], ["Done 1", "Reconnecting 1"]],
  [["interrupted"], ["Interrupted 1"]],
];

it.each(COUNTS)("counts the console's shown statuses for %j", (statuses, words) => {
  fakeTimers();
  const avatar = avatarOf(statuses);
  enter(avatar);
  expect(openTooltip()).toBeNull();
  runOutTimers();
  expect(tooltipRows()).toEqual(words);
  expect(openTooltip()!.querySelector("svg")).toBeNull();
  expect(avatar.getAttribute("aria-label")).toBe(`${isolate("Main")}, ${joinPhrases("en", words)}`);
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();
  leave(avatar);
  runOutTimers();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it.each([{ statuses: [] }, { statuses: ["archived"] }] as { statuses: Case[] }[])("tells the bare name with no unarchived sessions ($statuses)", ({ statuses }) => {
  fakeTimers();
  const avatar = avatarOf(statuses);
  enter(avatar);
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("Main");
  expect(avatar.getAttribute("aria-label")).toBe("Main");
  expect(avatar.closest('[data-slot="badge-anchor"]')!.children).toHaveLength(1);
  leave(avatar);
  runOutTimers();
});

it.each(["working", "starting", "dormant", "background", "delegated", "switching", "reconnecting", "interrupted"] as Case[])("counts %s even without an avatar badge", (status) => {
  const avatar = avatarOf([status]);
  expect(avatar.closest('[data-slot="badge-anchor"]')!.children).toHaveLength(1);
});

it("includes project sessions, excludes other consoles, and ignores archived project sessions", () => {
  fakeTimers();
  const avatar = avatarOf(["done"], false, [
    sessionOf("project-work", main.id, "project-1", "Work", "working"),
    sessionOf("other-console", "c-2", "project-2", "Other", "waiting_user"),
    sessionOf("project-archived", main.id, "project-1", "Old", "archived"),
  ]);
  enter(avatar);
  runOutTimers();
  expect(tooltipRows()).toEqual(["Done 1", "Working 1"]);
  leave(avatar);
  runOutTimers();
});

const EVERY_STATUS: Array<[Case, string]> = [
  ["waiting_user", "Waiting for you"],
  ["paused", "Paused"],
  ["done", "Done"],
  ["idle", "Awaiting instructions"],
];

it.each(EVERY_STATUS)("marks an avatar whose top status is %s with a badge, and names the status", (status, words) => {
  const avatar = avatarOf([status]);
  const badge = avatar.closest('[data-slot="badge-anchor"]')!.lastElementChild!;
  expect(badge.getAttribute("aria-hidden")).toBe("true");
  expect(badge.classList.contains("badge")).toBe(true);
  expect(badge.classList.contains("badge--bottom-right")).toBe(true);
  expect(avatar.getAttribute("aria-label")).toBe(`${isolate("Main")}, ${words} 1`);
});

it("gives the badge no tooltip or name of its own", () => {
  fakeTimers();
  const avatar = avatarOf(["waiting_user"]);
  const badge = avatar.closest('[data-slot="badge-anchor"]')!.lastElementChild!;

  enter(avatar);
  runOutTimers();
  // Only one tooltip is open at a time, so what shows is whose it is: the avatar's, not the badge's.
  expect(openTooltip()?.textContent).toBe("MainWaiting for you1");
  expect(badge.hasAttribute("aria-label")).toBe(false);
  leave(avatar);
  runOutTimers();
  expect(vi.getTimerCount()).toBe(0);
});

it("opens on keyboard focus, and not on hover while the hidden sidebar floats in", () => {
  fakeTimers();
  const avatar = avatarOf(["waiting_user"], true);

  enter(avatar);
  runOutTimers();
  expect(openTooltip()).toBeNull();
  leave(avatar);

  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    avatar.focus();
  });
  runOutTimers();
  expect(openTooltip()?.textContent).toBe("MainWaiting for you1");
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();

  act(() => avatar.blur());
  runOutTimers();
  expect(vi.getTimerCount()).toBe(0);
});
