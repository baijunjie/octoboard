// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { ConfigurationFileError } from "./configurationFileError";
import { tauriPlatform } from "./tauri";
import { UpdaterError } from "./updaterError";

const invoke = vi.fn();
/** The IPC channel the shell reports download progress through: the test calls `onmessage` itself. */
class FakeChannel<T> {
  onmessage: (message: T) => void = () => {};
}
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args), Channel: FakeChannel }));

afterEach(() => invoke.mockReset());

it.each([
  { shell: { kind: "too_large" }, kind: "too_large", detail: "" },
  { shell: { kind: "not_text" }, kind: "not_text", detail: "" },
  { shell: { kind: "io", detail: "Permission denied" }, kind: "io", detail: "Permission denied" },
  { shell: "something else", kind: "io", detail: "something else" },
])("tells a configuration file the shell refused by its kind: $shell", async ({ shell, kind, detail }) => {
  invoke.mockRejectedValue(shell);
  const error = await tauriPlatform().configurationFile!.open().catch((err: unknown) => err);
  expect(error).toBeInstanceOf(ConfigurationFileError);
  expect(error).toMatchObject({ kind, message: detail });
});

it("tells a configuration file the shell could not save by its kind too", async () => {
  invoke.mockRejectedValue({ kind: "io", detail: "Disk full" });
  const error = await tauriPlatform().configurationFile!.save("a.json", "{}").catch((err: unknown) => err);
  expect(error).toBeInstanceOf(ConfigurationFileError);
  expect(error).toMatchObject({ kind: "io", message: "Disk full" });
});

it("answers nothing for a dialog the user dismissed", async () => {
  invoke.mockResolvedValue(null);
  await expect(tauriPlatform().configurationFile!.open()).resolves.toBeUndefined();
  await expect(tauriPlatform().configurationFile!.save("a.json", "{}")).resolves.toBeUndefined();
});

it("asks the shell to relaunch only when told to", async () => {
  invoke.mockResolvedValue(undefined);
  await tauriPlatform().exit!.confirmQuit();
  await tauriPlatform().exit!.confirmQuit({ restart: true });
  expect(invoke.mock.calls).toEqual([
    ["confirm_quit", { restart: undefined }],
    ["confirm_quit", { restart: true }],
  ]);
});

it("hands the shell the channel to check and answers the version and notes it found", async () => {
  invoke.mockResolvedValue({ version: "0.2.0", notes: "Fixes." });
  await expect(tauriPlatform().updater!.check("stable")).resolves.toEqual({ version: "0.2.0", notes: "Fixes." });
  expect(invoke).toHaveBeenCalledWith("check_for_update", { channel: "stable" });
  invoke.mockResolvedValue({ version: "0.3.0", notes: null });
  await expect(tauriPlatform().updater!.check("beta")).resolves.toEqual({ version: "0.3.0", notes: undefined });
});

it("answers nothing for a check that found no newer version", async () => {
  invoke.mockResolvedValue(null);
  await expect(tauriPlatform().updater!.check("beta")).resolves.toBeUndefined();
});

it.each([
  { shell: { kind: "busy" }, kind: "busy", detail: "" },
  { shell: { kind: "installed" }, kind: "installed", detail: "" },
  { shell: { kind: "nothing_to_install" }, kind: "nothing_to_install", detail: "" },
  { shell: { kind: "failed", detail: "offline" }, kind: "failed", detail: "offline" },
  { shell: "something else", kind: "failed", detail: "something else" },
  { shell: { kind: "failed" }, kind: "failed", detail: "" },
  { shell: new Error("ipc down"), kind: "failed", detail: "ipc down" },
])("tells an update the shell refused by its kind: $shell", async ({ shell, kind, detail }) => {
  invoke.mockRejectedValue(shell);
  const error = await tauriPlatform().updater!.check("beta").catch((err: unknown) => err);
  expect(error).toBeInstanceOf(UpdaterError);
  expect(error).toMatchObject({ kind, message: detail });
});

it("passes the download's progress on while the shell installs", async () => {
  const progress = vi.fn();
  invoke.mockImplementation(async (_command: string, args: { onProgress: FakeChannel<{ downloaded: number; total: number | null }> }) => {
    args.onProgress.onmessage({ downloaded: 10, total: 100 });
    args.onProgress.onmessage({ downloaded: 20, total: null });
  });
  await tauriPlatform().updater!.install(progress);
  expect(invoke).toHaveBeenCalledWith("install_update", expect.anything());
  expect(progress.mock.calls).toEqual([[{ downloaded: 10, total: 100 }], [{ downloaded: 20, total: undefined }]]);
});

it("ignores progress the shell sends after the install has settled", async () => {
  const progress = vi.fn();
  let channel: FakeChannel<{ downloaded: number; total: number | null }> | undefined;
  invoke.mockImplementation(async (_command: string, args: { onProgress: FakeChannel<{ downloaded: number; total: number | null }> }) => {
    channel = args.onProgress;
  });
  await tauriPlatform().updater!.install(progress);
  channel!.onmessage({ downloaded: 30, total: 100 });
  expect(progress).not.toHaveBeenCalled();
});
