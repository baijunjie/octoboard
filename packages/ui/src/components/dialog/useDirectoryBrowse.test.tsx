// @vitest-environment jsdom
import { act } from "react";
import { expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter } from "../../platform";
import { mountHost } from "../../test/mountHost";
import { testDaemon } from "../../test/testDaemon";
import { errorAlerts } from "../alerts/errorAlerts";
import { ErrorAlerts } from "../../shell/ErrorAlerts";
import { focusAfterLoss, setFocusAfterModalLoss } from "../focus/modalFocus";
import { framesAsTimers, settleFrames } from "./dialogTestSupport";
import { Dialog } from "./Dialog";
import { useDirectoryBrowse } from "./DirectoryPicker";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Field({ value = " ~/code ", onPick = () => {}, onBlur = () => {} }: { value?: string; onPick?: (path: string) => void; onBlur?: (picking: boolean) => void }) {
  const browse = useDirectoryBrowse(value, onPick);
  return <><input aria-label="Path" value={value} readOnly onBlur={() => onBlur(browse.picking)} />{browse.browseButton({ preventFocusOnPress: true })}<output>{String(browse.picking)}</output>{browse.picker}</>;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function setup() {
  const result = deferred<string | undefined>();
  const pick = vi.fn(() => result.promise);
  const platform: PlatformAdapter = { kind: "tauri", directoryPicker: { pick } };
  const daemon = testDaemon({ isLocal: true });
  return { result, pick, platform, daemon };
}

it("uses the system picker only for a local desktop daemon, holding picking and ignoring repeat presses", async () => {
  const { result, pick, platform, daemon } = setup();
  const onPick = vi.fn();
  const host = mountHost(<Field onPick={onPick} />, { daemon, platform });
  const button = host.querySelector("button")!;
  await act(async () => { button.click(); button.click(); });
  expect(pick).toHaveBeenCalledExactlyOnceWith({ title: "Choose a directory", initialPath: "~/code" });
  expect(host.querySelector("output")?.textContent).toBe("true");
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  await act(async () => button.click());
  expect(pick).toHaveBeenCalledOnce();
  await act(async () => result.resolve("/chosen"));
  expect(onPick).toHaveBeenCalledExactlyOnceWith("/chosen");
  expect(host.querySelector("output")?.textContent).toBe("false");
});

it("commits picking before native focus leaves the field and restores its focus on cancellation", async () => {
  const { result, pick, platform, daemon } = setup();
  const onPick = vi.fn();
  const blur = vi.fn();
  const host = mountHost(<Field onPick={onPick} onBlur={blur} />, { daemon, platform });
  const input = host.querySelector("input")!;
  act(() => input.focus());
  pick.mockImplementation(() => { input.blur(); return result.promise; });
  await act(async () => host.querySelector("button")!.click());
  expect(blur).toHaveBeenCalledExactlyOnceWith(true);
  await act(async () => result.resolve(undefined));
  expect(onPick).not.toHaveBeenCalled();
  expect(input.value).toBe(" ~/code ");
  expect(document.activeElement).toBe(input);
});

it.each([
  { kind: "browser" as const, local: true, capability: true },
  { kind: "tauri" as const, local: false, capability: true },
  { kind: "tauri" as const, local: undefined, capability: true },
  { kind: "tauri" as const, local: true, capability: false },
])("keeps daemon browsing for $kind, local=$local, capability=$capability", async ({ kind, local, capability }) => {
  const pick = vi.fn();
  const request = vi.fn(async () => ({ type: "dir_listing" as const, id: null, path: "/remote", entries: [] }));
  const host = mountHost(<Field />, { daemon: testDaemon({ isLocal: local, request }), platform: { kind, directoryPicker: capability ? { pick } : undefined } });
  await act(async () => host.querySelector("button")!.click());
  expect(pick).not.toHaveBeenCalled();
  expect(request).toHaveBeenCalledWith({ type: "list_dir", path: "~/code" });
  expect(document.querySelector('[role="dialog"]')).not.toBeNull();
});

it("discards a native result after the field changes and blocks another opening until it settles", async () => {
  const { result, pick, platform, daemon } = setup();
  const onPick = vi.fn();
  const host = mountHost(<Field value="/first" onPick={onPick} />, { daemon, platform });
  await act(async () => host.querySelector("button")!.click());
  host.render(<Field value="/second" onPick={onPick} />);
  await act(async () => host.querySelector("button")!.click());
  expect(pick).toHaveBeenCalledOnce();
  await act(async () => result.resolve("/stale"));
  expect(onPick).not.toHaveBeenCalled();
  expect(host.querySelector("output")?.textContent).toBe("false");
});

it("ignores a result after unmount", async () => {
  const { result, platform, daemon } = setup();
  const onPick = vi.fn();
  const host = mountHost(<Field onPick={onPick} />, { daemon, platform });
  await act(async () => host.querySelector("button")!.click());
  host.render(null);
  await act(async () => result.resolve("/stale"));
  expect(onPick).not.toHaveBeenCalled();
});

it("reports a system picker failure through the shared error route and releases picking", async () => {
  const { result, platform, daemon } = setup();
  const report = vi.spyOn(errorAlerts, "report").mockImplementation(() => {});
  onTestFinished(() => report.mockRestore());
  const host = mountHost(<Field />, { daemon, platform });
  await act(async () => host.querySelector("button")!.click());
  await act(async () => result.reject(new Error("Picker failed")));
  expect(report).toHaveBeenCalledOnce();
  expect(report.mock.calls[0]![0].title).toBe("Could not open the directory");
  expect(host.querySelector("output")?.textContent).toBe("false");
});

it("lets the error alert take focus after a native failure and returns to the surrounding dialog on acknowledgement", async () => {
  framesAsTimers();
  onTestFinished(setFocusAfterModalLoss(() => focusAfterLoss(() => {})));
  const { result, pick, platform, daemon } = setup();
  mountHost(<><Dialog title="Edit project" onClose={() => {}} footer={null}><Field /></Dialog><ErrorAlerts daemon={daemon} /></>, { daemon, platform });
  onTestFinished(() => act(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); }));
  await settleFrames();
  const outer = document.querySelector<HTMLElement>('[role="dialog"]')!;
  const input = outer.querySelector("input")!;
  act(() => input.focus());
  pick.mockImplementation(() => { input.blur(); return result.promise; });
  await act(async () => [...outer.querySelectorAll("button")].find((button) => button.textContent === "Browse")!.click());
  await act(async () => result.reject(new Error("Picker failed")));
  expect(document.querySelector('[role="alertdialog"]')?.contains(document.activeElement)).toBe(true);
  // Outlast react-aria's mount-time focus correction before acknowledging, as a person reading the alert does.
  await act(() => new Promise((resolve) => setTimeout(resolve, 600)));
  await act(async () => errorAlerts.acknowledge());
  await settleFrames();
  await act(() => new Promise((resolve) => setTimeout(resolve, 450)));
  expect(outer.contains(document.activeElement)).toBe(true);
});
