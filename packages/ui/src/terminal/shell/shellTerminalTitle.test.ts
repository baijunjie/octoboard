import { describe, expect, it } from "vitest";

import { shellTerminalTitle } from "./shellTerminalTitle";

describe("shellTerminalTitle", () => {
  it.each([
    ["junjiedeMac-mini:/Volumes/Code/.worktrees/viewer-code-font", "viewer-code-font"],
    ["user@host:~/projects/octoboard", "octoboard"],
    ["host: /Volumes/My Projects/日本語/", "日本語"],
    ["/Volumes/Code/octoboard", "octoboard"],
    ["~/projects/octoboard/", "octoboard"],
    ["host:/", "/"],
    ["host:~/", "~"],
    ["zsh", "zsh"],
    ["npm run dev", "npm run dev"],
    ["vim /tmp/file", "vim /tmp/file"],
    ["/usr/bin/git -C /tmp/project status", "/usr/bin/git -C /tmp/project status"],
    ["~/bin/dev /tmp/project", "~/bin/dev /tmp/project"],
    ["/Volumes/My Projects/website", "/Volumes/My Projects/website"],
    ["Release: check /tmp/file", "Release: check /tmp/file"],
  ])("shows %s as %s", (title, expected) => {
    expect(shellTerminalTitle({ title, renamed: false })).toBe(expected);
  });

  it("keeps an explicitly chosen title even when it looks like a directory", () => {
    const title = "host:/projects/octoboard";
    expect(shellTerminalTitle({ title, renamed: true })).toBe(title);
  });
});
