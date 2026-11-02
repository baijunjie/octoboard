// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../fixtures/builders";
import { isolate } from "../i18n/isolate";
import { PlatformProvider } from "../platform/react";
import type { Session, SessionStatus } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { Rail } from "./Rail";
import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "../components/hint/tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-1", "Main");

/** A session status beyond the wire's: the ones derived from an idle session (`delegated` adds a
 * working session bound to it), and `switching` account over any status. */
type Case = SessionStatus | "background" | "paused" | "done" | "delegated" | "switching";

const EXTRAS: Partial<Record<Case, Partial<Session>>> = {
  background: { background_tasks: [{ id: "watcher" }] },
  paused: { paused: true },
  done: { has_conversation: true },
  switching: { switching_account: true },
};

/** Mounts a rail with one console, holding a session in each of `statuses`, whose avatar's pointer
 * hover floats the hidden sidebar in when `hoverOpensSidebar`. Returns the console's avatar button. */
function avatarOf(statuses: Case[], hoverOpensSidebar = false): HTMLButtonElement {
  const wire = (status: Case): SessionStatus => (status === "background" || status === "paused" || status === "done" || status === "delegated" || status === "switching" ? "idle" : status);
  const sessions = statuses.flatMap((status, index) => {
    const own = sessionOf(`s-${index}`, main.id, undefined, `Hub ${index}`, wire(status), { has_conversation: false, ...EXTRAS[status] });
    return status === "delegated" ? [own, sessionOf(`s-${index}-child`, main.id, undefined, `Child ${index}`, "working", { bound_to: own.id })] : [own];
  });
  const daemon = { store: createStateStore({ sessions: new Map(sessions.map((session) => [session.id, session])) }) } as Daemon;
  const container = mountForTooltip(
    <PlatformProvider value={{ kind: "browser" }}>
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

const AVATAR_TEXT: Array<[string, Case[], string]> = [
  ["a session is waiting for the user", ["working", "waiting_user"], "Waiting for you"],
  ["a session awaits instructions before another finishes working", ["idle", "working"], "Awaiting instructions"],
  ["a done session coexists with dormant and paused sessions", ["dormant", "paused", "done"], "Paused"],
  ["an idle session coexists with a starting session", ["starting", "idle"], "Awaiting instructions"],
  ["a waiting session takes priority over done", ["done", "paused", "waiting_user", "idle"], "Waiting for you"],
  ["a done session takes priority over awaiting instructions", ["idle", "done"], "Done"],
];

it.each(AVATAR_TEXT)("says on the avatar's tooltip, in its accessible name's words, that %s", (_case, statuses, words) => {
  const settle = withFakeTimers();
  const avatar = avatarOf(statuses);
  const name = `${isolate("Main")}, ${words}`;

  enter(avatar);
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()?.textContent).toBe(`Main, ${words}`);
  const statusBadge = openTooltip()?.querySelector('[data-variant="soft"]');
  expect(statusBadge?.textContent).toBe(words);
  expect(statusBadge?.getAttribute("data-color")).toBe(words === "Waiting for you" ? "warning" : "success");
  expect(statusBadge?.querySelector("svg")).toBeNull();
  expect(avatar.getAttribute("aria-label")).toBe(name);
  // The tooltip repeats the name, so it must not be announced a second time as a description.
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();

  leave(avatar);
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it.each<{ statuses: Case[] }>([[], ["archived"], ["working"], ["starting"], ["dormant"], ["background"], ["delegated"], ["switching"], ["interrupted"]].map((statuses) => ({ statuses: statuses as Case[] })))("tells the bare name without an avatar status for $statuses", ({ statuses }) => {
  const settle = withFakeTimers();
  const avatar = avatarOf(statuses);

  enter(avatar);
  settle();
  expect(openTooltip()?.textContent).toBe("Main");
  expect(avatar.getAttribute("aria-label")).toBe("Main");
  expect(avatar.querySelector("span.relative")!.children).toHaveLength(1);
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();

  leave(avatar);
  settle();
  expect(vi.getTimerCount()).toBe(0);
});

const EVERY_STATUS: Array<[Case, string]> = [
  ["waiting_user", "Waiting for you"],
  ["paused", "Paused"],
  ["done", "Done"],
  ["idle", "Awaiting instructions"],
];

it.each(EVERY_STATUS)("marks an avatar whose top status is %s with a badge, and names the status", (status, words) => {
  const avatar = avatarOf([status]);
  const badge = avatar.querySelector("span.relative")!.lastElementChild!;
  expect(badge.getAttribute("aria-hidden")).toBe("true");
  expect(badge.className).toContain("size-3.5");
  expect(avatar.getAttribute("aria-label")).toBe(`${isolate("Main")}, ${words}`);
});

it("gives the badge no tooltip or name of its own", () => {
  const settle = withFakeTimers();
  const avatar = avatarOf(["waiting_user"]);
  const badge = avatar.querySelector("span.relative")!.lastElementChild!;

  enter(badge);
  settle();
  // Only one tooltip is open at a time, so what shows is whose it is: the avatar's, not the badge's.
  expect(openTooltip()?.textContent).toBe("Main, Waiting for you");
  expect(badge.hasAttribute("aria-label")).toBe(false);
  leave(badge);
  settle();
  expect(vi.getTimerCount()).toBe(0);
});

it("opens on keyboard focus, and not on hover while the hidden sidebar floats in", () => {
  const settle = withFakeTimers();
  const avatar = avatarOf(["waiting_user"], true);

  enter(avatar);
  settle();
  expect(openTooltip()).toBeNull();
  leave(avatar);

  act(() => {
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
    avatar.focus();
  });
  settle();
  expect(openTooltip()?.textContent).toBe("Main, Waiting for you");
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();

  act(() => avatar.blur());
  settle();
  expect(vi.getTimerCount()).toBe(0);
});
