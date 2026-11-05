// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { t } from "../i18n/language";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/announce/StatusAnnouncer";
import { windowAttention } from "../components/announce/windowTestSupport";
import { LanguageProvider } from "../i18n/react";
import type { AvailableUpdate, PlatformAdapter, UpdaterCapability, UpdateProgress } from "../platform";
import { PlatformProvider } from "../platform/react";
import { UpdateNotice } from "./UpdateNotice";
import { updateChannel } from "./updateChannel";
import type { UpdateStore } from "./updateStore";
import { UpdatesProvider, useUpdates } from "./UpdatesProvider";
import { UpdaterError } from "../platform/updaterError";
import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../test/hosts";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The notice measures itself with one while it is shown, and jsdom has none of its own.
class FakeResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  updateChannel.set("beta");
});
afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

const FOUND: AvailableUpdate = { version: "0.2.0", notes: "Fixes and more fixes." };

function fakeUpdater() {
  let progress: (progress: UpdateProgress) => void = () => {};
  let finish: () => void = () => {};
  const install = vi.fn(
    (onProgress: (progress: UpdateProgress) => void) =>
      new Promise<void>((resolve) => {
        progress = onProgress;
        finish = resolve;
      }),
  );
  const updater: UpdaterCapability = {
    currentVersion: async () => "0.1.0",
    check: vi.fn(async () => FOUND),
    install,
  };
  return { updater, install, progress: (value: UpdateProgress) => progress(value), finish: () => finish() };
}

function mount(platform: PlatformAdapter, focusTerminal: () => void = () => {}) {
  let store: UpdateStore | undefined;
  const Capture = () => {
    store = useUpdates()?.store;
    return null;
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <PlatformProvider value={platform}>
        <LanguageProvider>
          <UpdatesProvider>
            <Capture />
            <UpdateNotice focusTerminal={focusTerminal} />
          </UpdatesProvider>
        </LanguageProvider>
      </PlatformProvider>,
    ),
  );
  return { container, store: () => store };
}

/** The text of `element` without the bidirectional isolates that wrap a version. */
const textOf = (element: Element) => (element.textContent ?? "").replace(/[\u2066-\u2069]/g, "");

const button = (container: HTMLElement, name: string) =>
  Array.from(container.querySelectorAll("button")).find((candidate) => candidate.textContent?.includes(name) || candidate.getAttribute("aria-label") === name);

/** Finds an update, as an automatic check would. */
async function find(store: UpdateStore | undefined) {
  await act(async () => {
    await store!.check("automatic");
  });
}

it("shows nothing where the platform cannot update itself", () => {
  const { container } = mount({ kind: "browser", host: BROWSER_HOST });
  expect(container.querySelector("[data-region=update]")).toBeNull();
});

it("shows nothing until an update is found", async () => {
  const { container } = mount({ kind: "tauri", host: BROWSER_HOST, updater: fakeUpdater().updater });
  await act(async () => {});
  expect(container.querySelector("[data-region=update]")).toBeNull();
});

it("tells the user a newer version is available and offers it without taking it", async () => {
  const { updater } = fakeUpdater();
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater });
  await find(store());
  const notice = container.querySelector<HTMLElement>("[data-region=update]")!;
  expect(notice.getAttribute("role")).toBe("region");
  expect(textOf(notice)).toContain("Version 0.2.0 is available.");
  expect(button(container, "Download and install")).toBeDefined();
  expect(updater.check).toHaveBeenCalledExactlyOnceWith("beta");
});

it.each([FOUND, { version: "0.2.0-beta.2" }])("opens the offered version's release page with or without inline notes: %j", async (update) => {
  const { updater } = fakeUpdater();
  updater.check = vi.fn(async () => update);
  const open = vi.fn(async () => {});
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater, links: { open } });
  await find(store());
  const link = container.querySelector<HTMLAnchorElement>("a[data-external-link]")!;
  const address = `https://github.com/baijunjie/octoboard/releases/tag/desktop-v${update.version}`;
  expect(link.textContent).toContain("What is new");
  expect(link.getAttribute("href")).toBe(address);
  expect(link.hasAttribute("aria-expanded")).toBe(false);
  expect(container.textContent).not.toContain("Fixes and more fixes.");
  await act(async () => link.click());
  expect(open).toHaveBeenCalledExactlyOnceWith(address);
  expect(container.textContent).not.toContain("Fixes and more fixes.");
});

it("hides the notice on Later", async () => {
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: fakeUpdater().updater });
  await find(store());
  await act(async () => button(container, "Later")!.click());
  expect(container.querySelector("[data-region=update]")).toBeNull();
});

it("downloads only when the user presses Download and install, with progress, and then asks for a restart", async () => {
  const fake = fakeUpdater();
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: fake.updater });
  await find(store());
  expect(fake.install).not.toHaveBeenCalled();
  await act(async () => button(container, "Download and install")!.click());
  expect(fake.install).toHaveBeenCalledOnce();
  // Until the shell reports the size, the bar only says the download is under way.
  expect(container.querySelector("[role=progressbar]")!.hasAttribute("aria-valuenow")).toBe(false);
  // Without a size there is no percentage to draw.
  expect(container.querySelector(".progress-bar__output")).toBeNull();
  act(() => fake.progress({ downloaded: 50, total: 100 }));
  expect(container.querySelector("[role=progressbar]")!.getAttribute("aria-valuenow")).toBe("50");
  expect(container.querySelector(".progress-bar__output")!.textContent).toBe("50%");
  expect(button(container, "Download and install")).toBeUndefined();
  await act(async () => fake.finish());
  expect(textOf(container)).toContain("Version 0.2.0 is installed. It takes effect after Octoboard restarts.");
  expect(button(container, "Restart now")).toBeDefined();
  expect(button(container, "Later")).toBeDefined();
});

async function installed(focusTerminal?: () => void) {
  const fake = fakeUpdater();
  const mounted = mount({ kind: "tauri", host: BROWSER_HOST, updater: fake.updater }, focusTerminal);
  await find(mounted.store());
  await act(async () => button(mounted.container, "Download and install")!.click());
  await act(async () => fake.finish());
  return mounted;
}

it.each([
  ["installer", WINDOWS_HOST, "Version 0.2.0 is downloaded. It is installed when Octoboard quits or restarts.", "An update is downloaded. It is installed when Octoboard quits or restarts."],
  ["in-place", MAC_HOST, "Version 0.2.0 is installed. It takes effect after Octoboard restarts.", "An update is installed. It takes effect after Octoboard restarts."],
])("words a finished download as the %s updater leaves it, with and without a known version", async (_updater, host, known, unknown) => {
  const fake = fakeUpdater();
  const withVersion = mount({ kind: "tauri", host, updater: fake.updater });
  await find(withVersion.store());
  await act(async () => button(withVersion.container, "Download and install")!.click());
  await act(async () => fake.finish());
  expect(textOf(withVersion.container)).toContain(known);
  const { updater } = fakeUpdater();
  updater.check = async () => { throw new UpdaterError("installed"); };
  const withoutVersion = mount({ kind: "tauri", host, updater });
  await find(withoutVersion.store());
  expect(textOf(withoutVersion.container)).toContain(unknown);
});

it("asks the window to restart on Restart now, and never restarts by itself", async () => {
  const { container, store } = await installed();
  const restart = vi.fn();
  store()!.onRestartRequested(restart);
  await act(async () => button(container, "Restart now")!.click());
  expect(restart).toHaveBeenCalledOnce();
});

it("only hides the notice on Later, and offers no further download", async () => {
  const { container, store } = await installed();
  await act(async () => button(container, "Later")!.click());
  expect(container.querySelector("[data-region=update]")).toBeNull();
  expect(store()!.getState()).toMatchObject({ phase: "installed", restartNoticeHidden: true });
  await act(async () => {
    await store()!.check("manual");
  });
  expect(container.querySelector("[data-region=update]")).toBeNull();
});

it("hands focus on when the button that held it takes the notice's buttons away", async () => {
  const focusTerminal = vi.fn();
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: fakeUpdater().updater }, focusTerminal);
  await find(store());
  // The Download button stays in the strip, which turns into the progress bar around it.
  act(() => button(container, "Download and install")!.focus());
  await act(async () => button(container, "Download and install")!.click());
  expect(button(container, "Download and install")).toBeUndefined();
  expect(focusTerminal).toHaveBeenCalledOnce();
});

it("hands focus on when Later hides the notice with focus on it", async () => {
  const focusTerminal = vi.fn();
  const { container } = await installed(focusTerminal);
  act(() => button(container, "Later")!.focus());
  await act(async () => button(container, "Later")!.click());
  expect(container.querySelector("[data-region=update]")).toBeNull();
  expect(focusTerminal).toHaveBeenCalledOnce();
});

it("publishes its height while it is shown and clears it as it goes", async () => {
  const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(42);
  onTestFinished(() => height.mockRestore());
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: fakeUpdater().updater });
  const published = () => document.documentElement.style.getPropertyValue("--update-notice-height");
  document.documentElement.style.removeProperty("--update-notice-height");
  await find(store());
  expect(published()).toBe("42px");
  await act(async () => button(container, "Later")!.click());
  expect(published()).toBe("0px");
});

it("announces what it says only once the window is attended, then after the focus-move delay", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const attend = windowAttention();
  attend(false);
  const { container, store } = mount({ kind: "tauri", host: BROWSER_HOST, updater: fakeUpdater().updater });
  const spoken = () => textOf(container.querySelector("[role=status]")!);
  await find(store());
  await act(() => vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS * 2));
  expect(spoken()).toBe("");
  attend(true);
  await act(() => vi.advanceTimersByTimeAsync(AFTER_FOCUS_MOVE_DELAY_MS - 1));
  expect(spoken()).toBe("");
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(spoken()).toBe("Version 0.2.0 is available.");
});

it("shows only the remedy for a generic installation failure", async () => {
  const { updater } = fakeUpdater();
  updater.install = async () => { throw new Error("network detail"); };
  const { store } = mount({ kind: "tauri", host: BROWSER_HOST, updater });
  await act(async () => { await store()!.check("manual"); });
  await act(async () => { await store()!.install(); });
  const alert = errorAlerts.getSnapshot().at(-1)!;
  expect(alert.title).toBe(t("updates.error.install.title"));
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  });
  act(() => root.render(<>{alert.content}</>));
  expect([...container.querySelectorAll("p")].map((line) => line.textContent)).toEqual([t("updates.error.install.remedy")]);
});
