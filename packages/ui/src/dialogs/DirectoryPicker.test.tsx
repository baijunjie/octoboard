// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { errorAlerts } from "../errorAlerts";
import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { ErrorAlerts } from "./ErrorAlerts";
import { DirectoryPicker } from "./DirectoryPicker";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Enter lists the path typed, which needs focus in the path field as the picker opens.
it("opens with focus in the path field", async () => {
  act(() => setInteractionModality("pointer"));
  const daemon: Daemon = {
    store: createStateStore(),
    request: vi.fn(async () => ({ type: "dir_listing", path: "/home", entries: [] })) as Daemon["request"],
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () =>
    root.render(
      <DaemonProvider value={daemon}>
        <DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
  expect(document.activeElement?.getAttribute("aria-label")).toBe("Directory path");
  // So that focus lost later, with an alert acknowledged, comes back here (`focusAfterLoss`).
  expect(document.activeElement?.hasAttribute("data-opening-focus")).toBe(true);
});

it("shows a listing failure in an Alert with its reason and Try again, and opens no error alert dialog", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const request = vi.fn().mockRejectedValueOnce(new Error("Permission denied")).mockResolvedValue({ type: "dir_listing", path: "/home", entries: [] });
  const daemon = { store: createStateStore(), request } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} /><ErrorAlerts daemon={daemon} /></DaemonProvider>));
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(errorAlerts.getSnapshot()).toEqual([]);
  const alert = document.querySelector('[data-slot="alert-root"]')!;
  expect(alert.querySelector('[data-slot="alert-title"]')?.textContent).toBe("Could not list this directory");
  expect(alert.textContent).toContain("Something unexpected went wrong.");
  expect(alert.textContent).not.toContain("Permission denied");
  // The first listing comes with the path field taking focus, so its announcement waits that out.
  expect(document.querySelector('[role="alert"]')).toBeNull();
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(document.querySelector('[role="alert"]')?.textContent).toContain("Something unexpected went wrong.");
  const retry = [...alert.querySelectorAll("button")].find((button) => button.textContent === "Try again")!;
  await act(async () => retry.click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(document.querySelector('[data-slot="alert-root"]')).toBeNull();
});

it("reports a failed Try again in the error alert dialog and leaves the first-listing Alert as it was, focus on Try again", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const request = vi.fn().mockRejectedValue(new Error("Permission denied"));
  const daemon = { store: createStateStore(), request } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} /></DaemonProvider>));
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(report).not.toHaveBeenCalled();
  const first = document.querySelector('[role="alert"]')!;
  const retry = [...document.querySelectorAll('[data-slot="alert-root"] button')].find((button) => button.textContent === "Try again") as HTMLButtonElement;
  act(() => retry.focus());
  await act(async () => retry.click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(report).toHaveBeenCalledOnce();
  expect(report.mock.calls[0]![0].title).toBe("Could not open the directory");
  // The dialog alone speaks the repeat: the Alert is the one from opening, neither replaced nor announced again.
  expect(document.querySelector('[role="alert"]')).toBe(first);
  expect(document.activeElement).toBe(retry);
});

it("reports a failed Go before any listing in the error alert dialog, and the first-listing Alert stays", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const request = vi.fn().mockRejectedValue(new Error("Permission denied"));
  const daemon = { store: createStateStore(), request } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} /></DaemonProvider>));
  expect(report).not.toHaveBeenCalled();
  await act(async () => [...document.querySelectorAll("button")].find((button) => button.textContent === "Go")!.click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(report).toHaveBeenCalledOnce();
  expect(report.mock.calls[0]![0].title).toBe("Could not open the directory");
  expect(document.querySelectorAll('[data-slot="alert-root"]')).toHaveLength(1);
});

it("keeps Try again where it is, pending and focused, while its listing is out", async () => {
  vi.useFakeTimers();
  onTestFinished(() => void vi.useRealTimers());
  let finish!: (error: Error) => void;
  const request = vi
    .fn()
    .mockRejectedValueOnce(new Error("Permission denied"))
    .mockReturnValueOnce(new Promise((_, reject) => void (finish = reject)));
  const daemon = { store: createStateStore(), request } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} /></DaemonProvider>));
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  const retry = [...document.querySelectorAll('[data-slot="alert-root"] button')].find((button) => button.textContent === "Try again") as HTMLButtonElement;
  act(() => retry.focus());
  await act(async () => retry.click());
  expect(request).toHaveBeenCalledTimes(2);
  expect(document.querySelector('[data-slot="alert-root"]')).not.toBeNull();
  expect(retry.isConnected).toBe(true);
  expect(retry.getAttribute("aria-disabled")).toBe("true");
  expect(retry.hasAttribute("data-pending")).toBe(true);
  expect(document.activeElement).toBe(retry);
  await act(async () => retry.click());
  expect(request).toHaveBeenCalledTimes(2);
  await act(async () => finish(new Error("Permission denied")));
  expect(retry.isConnected).toBe(true);
  expect(retry.getAttribute("aria-disabled")).not.toBe("true");
  expect(document.activeElement).toBe(retry);
});

/** A picker whose first listing succeeds and whose second fails, after which `navigate` is run. */
async function failLaterNavigation(navigate: () => void | Promise<void>) {
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const entries = [{ name: "code", path: "/home/me/code", is_git_repo: false }];
  const request = vi.fn()
    .mockResolvedValueOnce({ type: "dir_listing", path: "/home/me", entries })
    .mockRejectedValueOnce(new Error("Permission denied"));
  const daemon = { store: createStateStore(), request } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} /></DaemonProvider>));
  expect(option("code")).toBeDefined();
  await act(async () => navigate());
  expect(request).toHaveBeenCalledTimes(2);
  expect(report).toHaveBeenCalledOnce();
  expect(report.mock.calls[0]![0].title).toBe("Could not open the directory");
  // The failure is the error alert dialog's to show, not the picker's: no Alert, no mark, no loose text.
  expect(document.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(document.querySelector("[data-status-mark]")).toBeNull();
  expect(document.body.textContent).not.toContain("Could not list this directory");
  // The previous listing and its path stay.
  expect(option("code")).toBeDefined();
  expect((document.querySelector('input[aria-label="Directory path"]') as HTMLInputElement).value).toBe("/home/me");
}

const option = (name: string) => [...document.querySelectorAll('[role="option"]')].find((item) => item.getAttribute("aria-label") === name) as HTMLElement | undefined;

it("reports a failed later navigation to an entry in the error alert dialog and keeps the listing, with no Alert", () =>
  failLaterNavigation(() => option("code")!.click()));

it("reports a failed later navigation by Go in the error alert dialog and keeps the listing, with no Alert", () =>
  failLaterNavigation(() => [...document.querySelectorAll("button")].find((button) => button.textContent === "Go")!.click()));

it("reports a failed later navigation to the parent row in the error alert dialog and keeps the listing, with no Alert", () =>
  failLaterNavigation(() => option("Parent directory")!.click()));

async function mountListing(path: string, entries: unknown[]): Promise<HTMLElement> {
  const request = vi.fn(async () => ({ type: "dir_listing", path, entries }));
  const daemon = { store: createStateStore(), request } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><DirectoryPicker title="Choose directory" onPick={() => {}} onClose={() => {}} /></DaemonProvider>));
  return document.body;
}

it("shows an empty listing as the shared compact row in a disabled option, under the parent row", async () => {
  const body = await mountListing("/home/me", []);
  const options = [...body.querySelectorAll('[role="option"]')];
  expect(options.map((option) => option.getAttribute("aria-label") ?? option.textContent)).toEqual(["Parent directory", "No subdirectories"]);
  const empty = options[1]!;
  expect(empty.getAttribute("aria-disabled")).toBe("true");
  // The shared compact empty row: a glyph and a message in a plain row, with no heading and no button.
  expect(empty.querySelector('[data-slot="empty-state"] svg')).not.toBeNull();
  expect(empty.querySelector("h2, h3, button")).toBeNull();
});

it("shows an empty listing at the root as the only option, with no parent row", async () => {
  const body = await mountListing("/", []);
  const options = [...body.querySelectorAll('[role="option"]')];
  expect(options).toHaveLength(1);
  expect(options[0]!.textContent).toBe("No subdirectories");
  expect(options[0]!.getAttribute("aria-disabled")).toBe("true");
  expect(options[0]!.querySelector('[data-slot="empty-state"] svg')).not.toBeNull();
});
