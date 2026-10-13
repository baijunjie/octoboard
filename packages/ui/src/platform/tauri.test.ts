// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";

import { ConfigurationFileError } from "./configurationFileError";
import { tauriPlatform } from "./tauri";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

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
