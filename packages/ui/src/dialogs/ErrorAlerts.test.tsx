// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, onTestFinished, vi } from "vitest";

import { reportError } from "../reportError";
import { appConfig } from "../appConfig";
import { DaemonRequestError } from "../daemon-client";
import { ActionableError } from "../errorRecovery";
import { PlatformProvider } from "../platform/react";
import type { PlatformAdapter } from "../platform";
import { errorAlerts } from "../errorAlerts";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";
import { ErrorAlerts, issueUrl } from "./ErrorAlerts";
import { registerSettingsLauncher } from "../settings/settingsLauncher";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => { act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }); });

function mount(child?: React.ReactNode, platform: PlatformAdapter = { kind: "browser" }): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<PlatformProvider value={platform}>{child}<ErrorAlerts /></PlatformProvider>));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
}

it("keeps errors across host mounting, deduplicates queued copies and acknowledges one at a time", () => {
  errorAlerts.report({ message: "first", session: "one" });
  errorAlerts.report({ message: "first", session: "one" });
  errorAlerts.report({ message: "first", session: "two" });
  errorAlerts.report({ message: "last" });
  mount();
  expect(errorAlerts.getSnapshot()).toHaveLength(3);
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("first");
  act(() => document.querySelector<HTMLButtonElement>('[role="alertdialog"] button')?.click());
  expect(errorAlerts.getSnapshot()).toHaveLength(2);
  act(() => errorAlerts.acknowledge());
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("last");
  act(() => errorAlerts.acknowledge());
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("stays above an open form until acknowledged and leaves the form available for retry", () => {
  const close = vi.fn();
  mount(<Dialog title="Edit project" onClose={close} submitLabel="Save" onSubmit={() => {}}><input aria-label="Name" defaultValue="Kept" /></Dialog>);
  act(() => errorAlerts.report({ message: "Save failed" }));
  const alert = document.querySelector<HTMLElement>('[role="alertdialog"]')!;
  expect(alert).not.toBeNull();
  expect(alert.querySelector('[aria-label="Close"]')).toBeNull();
  expect(alert.contains(document.activeElement)).toBe(true);
  expect(document.activeElement).toBe(alert);
  act(() => alert.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  expect(document.querySelector('[role="alertdialog"]')).toBe(alert);
  expect(close).not.toHaveBeenCalled();
  act(() => alert.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", repeat: true, bubbles: true, cancelable: true })));
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  act(() => alert.querySelector<HTMLButtonElement>("button")?.click());
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
  expect(document.querySelector<HTMLInputElement>('input[aria-label="Name"]')?.value).toBe("Kept");
  expect(close).not.toHaveBeenCalled();
});

it("reports an identical immediate failure again after acknowledgement and retry", async () => {
  const confirm = vi.fn().mockRejectedValue(new Error("Refused"));
  mount(<ConfirmDialog title="Save project" message="Save it?" onConfirm={confirm} onCancel={() => {}} />);
  const submit = () => document.querySelector('[role="alertdialog"] form')!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  await act(async () => { submit(); });
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  act(() => errorAlerts.acknowledge());
  await act(async () => { submit(); });
  expect(confirm).toHaveBeenCalledTimes(2);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
  expect(errorAlerts.getSnapshot()[0].message).toBe("Refused");
});

it("gives known disk and lock failures specific remedies without bug-report instructions", () => {
  mount();
  act(() => errorAlerts.report({ message: "No space left on device (os error 28)", error: new DaemonRequestError("No space left on device (os error 28)", "internal_error", {}) }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Free space on the disk");
  expect(document.querySelector('[role="alertdialog"]')?.textContent).not.toContain("Take a screenshot");
  act(() => errorAlerts.acknowledge());
  act(() => errorAlerts.report({ message: "Session locked", error: new DaemonRequestError("Session locked", "session_locked", { session: "s1" }) }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("choose Unlock");
});

it("offers screenshot and Issue guidance for unexpected failures, opening only after a user click", async () => {
  let opened!: () => void;
  const open = vi.fn(() => new Promise<void>((resolve) => { opened = resolve; }));
  mount(undefined, { kind: "browser", links: { open } });
  const source = new Error("Renderer invariant failed");
  act(() => errorAlerts.report({ message: source.message, error: source }));
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("Take a screenshot");
  expect(alert.textContent).toContain("Renderer invariant failed");
  expect(open).not.toHaveBeenCalled();
  const report = [...alert.querySelectorAll("a")].find((link) => link.textContent === "Report issue")!;
  expect(report.getAttribute("href")).toBe(appConfig.issueUrl);
  expect(report.classList.contains("button--secondary")).toBe(true);
  expect(alert.querySelector("p [data-inline-chip]")?.textContent).toBe("Report issue");
  expect(alert.querySelector("p a")).toBeNull();
  expect(report.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
  expect(alert.querySelectorAll('a[href="' + appConfig.issueUrl + '"]')).toHaveLength(1);
  await act(async () => report.click());
  expect(open).toHaveBeenCalledExactlyOnceWith(issueUrl);
  expect(report.getAttribute("aria-busy")).toBe("true");
  await act(async () => report.click());
  expect(open).toHaveBeenCalledTimes(1);
  await act(async () => opened());
  expect(report.getAttribute("aria-busy") === "true").toBe(false);
  expect(errorAlerts.getSnapshot()).toHaveLength(1);
});

it("immediately shows an actionable manual URL fallback if opening an Issue fails", async () => {
  const open = vi.fn().mockRejectedValue(Object.assign(new Error("Browser unavailable"), { code: "EACCES" }));
  mount(undefined, { kind: "tauri", links: { open } });
  act(() => errorAlerts.report({ message: "Unexpected failure" }));
  const report = [...document.querySelectorAll("a")].find((link) => link.textContent === "Report issue")!;
  await act(async () => report.click());
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("The Issue page could not be opened.");
  expect(alert.textContent).toContain(issueUrl);
  expect(alert.textContent).toContain("open it manually");
  expect([...alert.querySelectorAll("a")].some((link) => link.textContent === "Report issue")).toBe(false);
  expect(errorAlerts.getSnapshot()).toHaveLength(2);
  act(() => errorAlerts.acknowledge());
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Unexpected failure");
});

it("retains explicit clipboard recovery metadata through the queue", () => {
  mount();
  const error = new ActionableError("Copy failed", "clipboard");
  act(() => errorAlerts.report({ message: error.message, error }));
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Allow clipboard access");
});

it.each([
  ["connection", ["Try again"]],
  ["path", ["Browse"]],
  ["unlock", ["Unlock"]],
] as const)("renders %s recovery control references as chips", (recovery, labels) => {
  mount();
  act(() => errorAlerts.report({ message: "Action failed", recovery }));
  const chips = [...document.querySelectorAll('[role="alertdialog"] [data-inline-chip]')].map((chip) => chip.textContent);
  expect(chips).toEqual(labels);
});

it.each([
  ["account", "Manage accounts…", undefined],
  ["addedAgents", "Manage accounts…", undefined],
  ["agentNotAdded", "Add agent…", true],
] as const)("offers a button for %s recovery that opens Settings' Agent accounts", (recovery, label, addAgent) => {
  mount();
  const open = vi.fn();
  onTestFinished(registerSettingsLauncher(open));
  act(() => errorAlerts.report({ message: "Action failed", recovery }));
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).find((b) => b.textContent === label);
  expect(button).toBeDefined();
  act(() => button!.click());
  expect(open).toHaveBeenCalledWith("accounts", { addAgent });
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

it("offers the footer action for a batch only when every failure shares that recovery", () => {
  mount();
  const labels = () => Array.from(document.querySelectorAll<HTMLButtonElement>('[role="alertdialog"] button')).map((b) => b.textContent);
  act(() => errorAlerts.report({ message: "Batch", failures: [{ error: "a", recovery: "account" }, { error: "b", recovery: "account" }] }));
  expect(labels()).toContain("Manage accounts…");
  act(() => errorAlerts.acknowledge());
  act(() => errorAlerts.report({ message: "Mixed", failures: [{ error: "a", recovery: "account" }, { error: "b", recovery: "path" }] }));
  expect(labels()).not.toContain("Manage accounts…");
});

it("hands focus to the window's fallback when acknowledging the last alert leaves it on the page body", async () => {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  mount();
  const fallback = vi.fn();
  onTestFinished(errorAlerts.setFocusFallback(fallback));
  act(() => errorAlerts.report({ message: "Plain", recovery: "none" }));
  expect(document.querySelector('[role="alertdialog"] p')).toBeNull();
  act(() => errorAlerts.acknowledge());
  (document.activeElement as HTMLElement | null)?.blur();
  await act(async () => { vi.advanceTimersByTime(500); });
  expect(fallback).toHaveBeenCalledOnce();
});

it("shows foreign-realm diagnostic stacks and an accurate manual Issue fallback", () => {
  mount(undefined, { kind: "tauri" });
  const error = { message: "Foreign failure", stack: "Foreign stack trace" };
  act(() => errorAlerts.report({ message: error.message, error }));
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.querySelector("details")?.textContent).toContain("Foreign stack trace");
  expect(alert.textContent).toContain("open https://github.com/baijunjie/octoboard/issues/new in your browser");
  expect(alert.textContent).toContain(issueUrl);
  expect(alert.textContent).not.toContain("use Report issue");
});

it.each(["Module load reason", { message: "Module load reason" }])("shows original diagnostics when a generic failure summary has no source stack: %s", (error) => {
  mount();
  act(() => errorAlerts.report({ message: "Formula rendering failed", content: <p>Formula rendering failed</p>, error }));
  expect(document.querySelector('[role="alertdialog"] details')?.textContent).toContain("Module load reason");
});

it("preserves an unhandled foreign rejection's message and stack through the reporting normalizer", () => {
  mount();
  act(() => reportError({ message: "Foreign rejection", stack: "Foreign rejection stack" }));
  const alert = document.querySelector('[role="alertdialog"]')!;
  expect(alert.textContent).toContain("Foreign rejection");
  expect(alert.querySelector("details")?.textContent).toContain("Foreign rejection stack");
});
