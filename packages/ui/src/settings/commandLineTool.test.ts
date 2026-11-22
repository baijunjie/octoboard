import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { expect, it } from "vitest";

import { COMMAND_LINE_DIRECTORIES, COMMAND_LINE_TOOL } from "./commandLineTool";

// The desktop shell is what makes the link, so the directories and the name the Settings row states are the shell's:
// read from its source, since the UI cannot import it.
const shell = readFileSync(fileURLToPath(new URL("../../../../apps/desktop/src-tauri/src/command_line/macos.rs", import.meta.url)), "utf8");

it("names the tool as the shell names the link", () => {
  expect(/^const NAME: &str = "([^"]+)";$/m.exec(shell)?.[1]).toBe(COMMAND_LINE_TOOL);
});

it("states the directories the shell links in, the system's and the user's own", () => {
  const places = /fn in_home\([\s\S]*?Self \{([\s\S]*?)\}/.exec(shell)?.[1] ?? "";
  expect(/system: PathBuf::from\("([^"]+)"\)/.exec(places)?.[1]).toBe(COMMAND_LINE_DIRECTORIES.system);
  // The user's directory is built from the home directory, part by part.
  const user = /user: home((?:\s*\.join\("[^"]+"\))+)/.exec(places)?.[1] ?? "";
  const parts = [...user.matchAll(/\.join\("([^"]+)"\)/g)].map((match) => match[1]);
  expect(parts.length).toBeGreaterThan(0);
  expect(["~", ...parts].join("/")).toBe(COMMAND_LINE_DIRECTORIES.user);
});
