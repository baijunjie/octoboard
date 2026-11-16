import { expect, it } from "vitest";

import extensions from "../../../config/launchable-extensions.json";
import { isLaunchableName } from "./launchable";

it("knows every extension of the lists that hold everywhere, and those of the Windows list only for a Windows system", () => {
  const everywhere = [...extensions.anyComponent, ...extensions.finalFile];
  expect(everywhere.length + extensions.windowsFinalFile.length).toBeGreaterThan(90);
  for (const windows of [false, true]) for (const extension of everywhere) expect(isLaunchableName(`file.${extension}`, windows), extension).toBe(true);
  for (const extension of extensions.windowsFinalFile) {
    expect(isLaunchableName(`file.${extension}`, true), extension).toBe(true);
    expect(isLaunchableName(`file.${extension}`, false), extension).toBe(false);
  }
});

it("takes a script a Windows system runs for text where the system is another: .js opens in an editor on macOS and Linux", () => {
  expect(isLaunchableName("app.js", true)).toBe(true);
  expect(isLaunchableName("app.js", false)).toBe(false);
  expect(isLaunchableName(".js", false)).toBe(false);
  expect(isLaunchableName("setup.lnk", false)).toBe(false);
});

it("names the shell scripts, installers and bundles the system would run, and not the files it shows", () => {
  for (const name of ["run.sh", "Setup.EXE", "tool.py", "App.app", "x.lnk", "a.pdf.exe", "start.command", "Loud.COMMAND"]) expect(isLaunchableName(name, true), name).toBe(true);
  for (const name of ["notes.txt", "a.pdf", "photo.png", "a.exe.pdf", "script.sh.txt", "a.ts", "README.md"]) expect(isLaunchableName(name, true), name).toBe(false);
});

it("judges what follows the last dot even when it is the first character, and no name without a dot as launchable", () => {
  for (const name of [".sh", ".exe", ".bat", ".LNK", ".cmd", ".hta", "a.exe.", "a.exe "]) expect(isLaunchableName(name, true), name).toBe(true);
  for (const name of ["exe", "tool", "Makefile", "", ".", "..", ".gitignore"]) expect(isLaunchableName(name, true), name).toBe(false);
});
