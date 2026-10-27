// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import { PlatformProvider } from "../platform/react";
import type { SessionStatus } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { Rail } from "./Rail";
import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "./tooltipTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const main = consoleOf("c-1", "Main");

/** Mounts a rail with one console, holding a session in each of `statuses`, whose avatar's pointer
 * hover floats the hidden sidebar in when `hoverOpensSidebar`. Returns the console's avatar button. */
function avatarOf(statuses: SessionStatus[], hoverOpensSidebar = false): HTMLButtonElement {
  const sessions = statuses.map((status, index) => sessionOf(`s-${index}`, main.id, undefined, `Hub ${index}`, status));
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

const AVATAR_TEXT: Array<[string, SessionStatus[], string]> = [
  ["a session is waiting for the user", ["working", "waiting_user"], "a session is waiting for you"],
  ["a session is working", ["idle", "working"], "a session is working"],
  ["a session is merely running", ["idle"], "a session is running"],
];

it.each(AVATAR_TEXT)("says on the avatar's tooltip, in its accessible name's words, that %s", (_case, statuses, words) => {
  const settle = withFakeTimers();
  const avatar = avatarOf(statuses);
  const name = `${isolate("Main")}, ${words}`;

  enter(avatar);
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()?.textContent).toBe(name);
  expect(avatar.getAttribute("aria-label")).toBe(name);
  // The tooltip repeats the name, so it must not be announced a second time as a description.
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();

  leave(avatar);
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("tells the bare name where nothing is going on", () => {
  const settle = withFakeTimers();
  const avatar = avatarOf([]);

  enter(avatar);
  settle();
  expect(openTooltip()?.textContent).toBe("Main");
  expect(avatar.getAttribute("aria-label")).toBe("Main");
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();

  leave(avatar);
  settle();
  expect(vi.getTimerCount()).toBe(0);
});

it("gives the badge no tooltip or name of its own", () => {
  const settle = withFakeTimers();
  const avatar = avatarOf(["waiting_user"]);
  const badge = avatar.querySelector("span.relative")!.lastElementChild!;

  enter(badge);
  settle();
  // Only one tooltip is open at a time, so what shows is whose it is: the avatar's, not the badge's.
  expect(openTooltip()?.textContent).toBe(`${isolate("Main")}, a session is waiting for you`);
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
  expect(openTooltip()?.textContent).toBe(`${isolate("Main")}, a session is waiting for you`);
  expect(avatar.getAttribute("aria-describedby")).toBeFalsy();

  act(() => avatar.blur());
  settle();
  expect(vi.getTimerCount()).toBe(0);
});
