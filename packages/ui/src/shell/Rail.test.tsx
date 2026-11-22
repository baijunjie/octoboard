// @vitest-environment jsdom
import { act, type ReactElement } from "react";
import { expect, it, vi } from "vitest";

import { consoleOf, sessionOf } from "../fixtures/builders";
import { joinPhrases } from "../i18n/joinPhrases";
import { isolate } from "../i18n/isolate";
import type { PlatformAdapter, RemoteDaemonStatus } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { Session, SessionStatus } from "../protocol";
import { DaemonProvider } from "../store";
import { testDaemon } from "../test/testDaemon";
import { RemoteDevicesTestHost } from "../remoteDaemons/remoteDevicesTestSupport";
import { FIXED_SMALL_ICON_BUTTONS_CLASS } from "../primitives";
import { CHROME_BUTTON_FILLS, CHROME_CURRENT_FILLS } from "./ChromeButton";
import { Rail } from "./Rail";
import { enter, leave, mountForTooltip, openTooltip } from "../components/hint/tooltipTestSupport";
import { BROWSER_HOST, MAC_HOST } from "../test/hosts";
import { fakeTimers, runOutTimers } from "../test/clock";
import { DEFAULT_SETTINGS } from "../store/defaultSettings";

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

type RailProps = Parameters<typeof Rail>[0];

/** Mounts the rail with one console current and every handler a no-op, over a browser platform and a default daemon unless
 * given others; `props` override the rail's props, and `wrap` puts a host around it. */
function mountRail({
  props,
  platform = { kind: "browser", host: BROWSER_HOST },
  daemon = testDaemon(),
  wrap = (rail) => rail,
}: {
  props?: Partial<RailProps>;
  platform?: PlatformAdapter;
  daemon?: ReturnType<typeof testDaemon>;
  wrap?: (rail: ReactElement) => ReactElement;
} = {}): HTMLElement {
  return mountForTooltip(
    <PlatformProvider value={platform}>
      <DaemonProvider value={daemon}>
        {wrap(
          <Rail
            consoles={[main]}
            currentConsoleId={main.id}
            onSelectConsole={() => {}}
            onConsoleEnter={() => {}}
            onConsoleLeave={() => {}}
            hoverOpensSidebar={false}
            remoteDevicesShown={false}
            onOpenRemoteDevices={() => {}}
            onOpenScheduledTasks={() => {}}
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
            {...props}
          />,
        )}
      </DaemonProvider>
    </PlatformProvider>,
  );
}

/** Mounts a rail with one console, holding a session in each of `statuses`, whose avatar's pointer
 * hover floats the hidden sidebar in when `hoverOpensSidebar`. Returns the console's avatar button. */
function avatarOf(statuses: Case[], hoverOpensSidebar = false, otherSessions: Session[] = []): HTMLButtonElement {
  const wire = (status: Case): SessionStatus => (status === "background" || status === "paused" || status === "done" || status === "delegated" || status === "switching" || status === "reconnecting" ? "idle" : status);
  const sessions = statuses.flatMap((status, index) => {
    const own = sessionOf(`s-${index}`, main.id, undefined, `Hub ${index}`, wire(status), { has_conversation: false, ...EXTRAS[status] });
    return status === "delegated" ? [own, sessionOf(`s-${index}-child`, main.id, undefined, `Child ${index}`, "working", { bound_to: own.id })] : [own];
  });
  const daemon = testDaemon({ state: { sessions: new Map([...sessions, ...otherSessions].map((session) => [session.id, session])) } });
  const container = mountRail({ daemon, props: { hoverOpensSidebar } });
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
  // The console's name is the subject of the tooltip, so its heading stays emphasized.
  expect(openTooltip()!.querySelector("hr")!.previousElementSibling!.classList.contains("font-medium")).toBe(true);
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

/** The rail in the application's own window, with the remote daemons the shell reports connected. */
async function railWithRemoteDaemons(connected: RemoteDaemonStatus[]): Promise<HTMLElement> {
  const entry = (id: string, name: string) => ({ id, name, host: `${id}.example.com`, port: 22, user: "u", auth: "device" as const, keyPath: null, fingerprint: null, octoboarddPath: null, deviceKey: { declined: false, file: null, mark: null } });
  const platform: PlatformAdapter = {
    kind: "tauri",
    host: MAC_HOST,
    remoteDaemons: {
      list: async () => [entry("staging", "Staging"), entry("lab", "Home lab")],
      status: async () => connected,
      onChanged: () => () => {},
      save: async (saved) => ({ entry: { ...saved, id: saved.id ?? "new", deviceKey: { declined: false, file: null, mark: null } }, keyRemoval: { line: { result: "not_installed" as const }, privateKeyLeft: null } }),
      remove: async () => ({ line: { result: "not_installed" as const }, privateKeyLeft: null }),
      setDeviceKey: async () => ({ line: { result: "not_installed" as const }, privateKeyLeft: null }),
      connect: async () => ({ outcome: "opened" }),
      disconnect: async () => {},
      onPageRequested: () => () => {},
      onHandedOver: () => () => {},
      showMainWindow: async () => {},
      showUpdates: async () => {},
    },
  };
  let container!: HTMLElement;
  await act(async () => {
    container = mountRail({ platform, wrap: (rail) => <RemoteDevicesTestHost capability={platform.remoteDaemons}>{rail}</RemoteDevicesTestHost> });
  });
  return container;
}

const FOLLOWING = Node.DOCUMENT_POSITION_FOLLOWING;

it("puts the Remote devices button below the consoles and above the settings, under Scheduled tasks behind a groove", async () => {
  fakeTimers();
  const container = await railWithRemoteDaemons([{ id: "staging", state: "connected", via: "window" }]);
  const remote = container.querySelector<HTMLElement>("button[aria-label^='Remote devices']")!;
  expect(remote).not.toBeNull();
  const consoleAvatar = container.querySelector("[data-sidebar-peek-keep]")!;
  const newConsole = container.querySelector("button[aria-label='New console']")!;
  const settings = container.querySelector("button[aria-label='Settings']")!;
  expect(consoleAvatar.compareDocumentPosition(remote) & FOLLOWING).toBeTruthy();
  expect(newConsole.compareDocumentPosition(remote) & FOLLOWING).toBeTruthy();
  expect(remote.compareDocumentPosition(settings) & FOLLOWING).toBeTruthy();
  // Scheduled tasks stands directly above it, and the groove above the two is an item of the rail's stack of its own,
  // not their group's padding.
  const nav = remote.closest("nav")!;
  const items = [...nav.children];
  const tasks = items[items.findIndex((item) => item.contains(remote)) - 1];
  expect(tasks.getAttribute("aria-label") ?? tasks.querySelector("button")?.getAttribute("aria-label")).toBe("Scheduled tasks");
  const above = items[items.indexOf(tasks) - 1];
  expect(above.classList.contains("groove-t")).toBe(true);
  expect(above.textContent).toBe("");
  // One button for every connection held, which its name counts.
  expect(remote.getAttribute("aria-label")).toBe("Remote devices, connected: 1");
  expect(container.querySelectorAll("button[aria-label*='Staging']").length).toBe(0);
});

it("puts the gap that draws a groove above the bottom group between it and the group of pages, in every window", async () => {
  fakeTimers();
  const container = await railWithRemoteDaemons([]);
  const gap = container.querySelector("[data-rail-gap]");
  const remote = container.querySelector("button[aria-label^='Remote devices']")!;
  const settings = container.querySelector("button[aria-label='Settings']")!;
  expect(gap).not.toBeNull();
  expect(remote.compareDocumentPosition(gap!) & FOLLOWING).toBeTruthy();
  expect(gap!.compareDocumentPosition(settings) & FOLLOWING).toBeTruthy();
  // The separator under the space is an item of the rail's stack, directly followed by the bottom group.
  const groove = container.querySelector("[data-rail-groove]")!;
  expect(gap!.nextElementSibling).toBe(groove);
  expect(groove.parentElement).toBe(gap!.parentElement);
  // A browser has no Remote devices, and the gap goes under Scheduled tasks.
  const browser = avatarOf(["idle"]).closest("nav")!;
  const tasks = browser.querySelector("button[aria-label='Scheduled tasks']")!;
  expect(tasks.compareDocumentPosition(browser.querySelector("[data-rail-gap]")!) & FOLLOWING).toBeTruthy();
});

it("keeps every item of the rail directly in its one stack, with no group wrapper adding space of its own", async () => {
  fakeTimers();
  const container = await railWithRemoteDaemons([]);
  const nav = container.querySelector("nav")!;
  for (const name of ["Remote devices", "Settings"]) {
    const button = nav.querySelector(`button[aria-label^='${name}']`)!;
    // Whatever wraps a button between it and the nav adds no padding or margin.
    for (let wrapper: Element | null = button; wrapper && wrapper !== nav; wrapper = wrapper.parentElement) {
      expect(wrapper.className).not.toMatch(/(^|\s)(p[xytblr]?|m[xytblr]?)-\d/);
    }
  }
});

it("keeps the button while no remote daemon is connected, named without a count", async () => {
  fakeTimers();
  const container = await railWithRemoteDaemons([]);
  expect(container.querySelector("button[aria-label='Remote devices']")).not.toBeNull();
  expect(container.querySelector("button[aria-label='Settings']")).not.toBeNull();
});

it("has no remote daemons in a browser", async () => {
  fakeTimers();
  const rail = avatarOf(["idle"]).closest("nav")!;
  expect(rail.querySelector("button[aria-label^='Remote devices']")).toBeNull();
});

it("puts the Forwarded ports button of a remote window above the settings, only while a port is forwarded, and none in a browser", async () => {
  fakeTimers();
  vi.stubGlobal("__TAURI_INTERNALS__", {});
  window.history.replaceState(null, "", "/?remote=build-server&daemon=me%40build-server");
  const forwards = { list: async () => [{ remotePort: 3000, localPort: 3000, address: "http://localhost:3000/", localAddress: "http://localhost:3000/" }], stop: async () => {}, onChange: () => () => {} };
  let container!: HTMLElement;
  await act(async () => {
    container = mountRail({ platform: { kind: "tauri", host: BROWSER_HOST, remoteForwards: forwards } });
  });
  const button = container.querySelector<HTMLElement>("#rail-forwards")!;
  expect(button.getAttribute("aria-label")).toBe("1 port is forwarded. Show it.");
  const nav = button.closest("nav")!;
  expect(nav.querySelector("button[aria-label='Settings']")!.compareDocumentPosition(button) & FOLLOWING).toBeFalsy();
  expect(nav.querySelector("[data-sidebar-peek-keep]")!.compareDocumentPosition(button) & FOLLOWING).toBeTruthy();
  vi.unstubAllGlobals();
  window.history.replaceState(null, "", "/");
  expect(avatarOf(["idle"]).closest("nav")!.querySelector("#rail-forwards")).toBeNull();
});

// A console avatar's context menu follows the memory setting, which the rail reads from the store.
it.each([true, false])("offers Memory in a console avatar's context menu only while the memory setting is on (on: %s)", (on) => {
  fakeTimers();
  const daemon = testDaemon({ state: { settings: { ...DEFAULT_SETTINGS, memory_enabled: on } } });
  const container = mountRail({ daemon });
  const tile = container.querySelector<HTMLElement>("[data-sidebar-peek-keep]")!.closest<HTMLElement>("[data-list-motion]")!;
  act(() => void tile.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 4, clientY: 4 })));
  const labels = [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].map((el) => el.querySelector(".label")?.textContent ?? el.textContent);
  expect(labels).toContain("Console settings");
  expect(labels.includes("Memory")).toBe(on);
});

const avatarButton = (container: HTMLElement) => container.querySelector<HTMLButtonElement>("[data-sidebar-peek-keep]")!;
const classesOf = (element: Element) => element.className.split(/\s+/);

it("puts every rail button under the fixed small icon-button size, and gives the stack 8px above and below", () => {
  const nav = mountRail().querySelector("nav")!;
  // HeroUI grows a small icon-only button to 36px below its `md` breakpoint; the nav holds them all at 32px.
  expect(classesOf(nav)).toContain(FIXED_SMALL_ICON_BUTTONS_CLASS);
  expect(classesOf(nav)).toContain("py-2");
  expect(classesOf(nav)).not.toContain("pt-1");
  // The buttons the pin is meant for are inside the nav.
  for (const name of ["New console", "Scheduled tasks", "Settings"]) {
    expect(nav.querySelector(`button[aria-label='${name}']`)).not.toBeNull();
  }
  expect(nav.contains(avatarButton(nav))).toBe(true);
});

it("draws the current console's avatar on the current fills, in a round button with the avatar inside it", () => {
  const avatar = avatarButton(mountRail());
  expect(avatar.getAttribute("aria-current")).toBeTruthy();
  // The same fills as the rail's current page buttons and the top bar's open panel toggles.
  for (const fill of CHROME_CURRENT_FILLS.split(" ")) expect(classesOf(avatar)).toContain(fill);
  // Literal, so the check holds if the constant loses its resting fill: the current tint at rest.
  expect(classesOf(avatar)).toContain("[--button-bg:var(--chrome-active)]");
  // Not the interactive fills' own: a hover tint, which a current console does not take.
  expect(classesOf(avatar)).not.toContain("[--button-bg-hover:var(--chrome-hover)]");
  // No ring is drawn.
  expect(avatar.className).not.toContain("after:");
  // A round 32px button around a 24px avatar, so the fill shows as a 4px band.
  expect(classesOf(avatar)).toContain("size-8");
  expect(classesOf(avatar)).toContain("rounded-full");
  const face = avatar.querySelector("span[aria-hidden='true']")!;
  expect(classesOf(face)).toContain("size-6");
  expect(classesOf(face)).not.toContain("size-8");
  expect(classesOf(face)).toContain("rounded-full");
});

it("leaves a console that is not current on the chrome's hover and pressed fills", () => {
  const avatar = avatarButton(mountRail({ props: { currentConsoleId: "c-other" } }));
  expect(avatar.hasAttribute("aria-current")).toBe(false);
  for (const fill of CHROME_BUTTON_FILLS.split(" ")) expect(classesOf(avatar)).toContain(fill);
  // No resting fill, which is the current one's.
  expect(classesOf(avatar)).not.toContain("[--button-bg:var(--chrome-active)]");
  // A pointer press and a keyboard press show the pressed fill, not the hover one HeroUI's rule order would keep.
  expect(classesOf(avatar)).toContain("active:[--button-bg-hover:var(--chrome-active)]");
  expect(classesOf(avatar)).toContain("ui-pressed:[--button-bg-hover:var(--chrome-active)]");
  expect(avatar.className).not.toContain("after:");
  expect(classesOf(avatar)).toContain("size-8");
  expect(classesOf(avatar)).toContain("rounded-full");
  const face = avatar.querySelector("span[aria-hidden='true']")!;
  expect(classesOf(face)).toContain("size-6");
});

it("marks no console as current while the Scheduled tasks page shows", () => {
  const avatar = avatarButton(mountRail({ props: { scheduledTasksShown: true } }));
  expect(avatar.hasAttribute("aria-current")).toBe(false);
  expect(classesOf(avatar)).not.toContain("[--button-bg:var(--chrome-active)]");
});
