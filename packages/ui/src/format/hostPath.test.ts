import { describe, expect, it } from "vitest";

import {
  basename,
  display,
  expandHome,
  hostPathsOf,
  isAbsolute,
  isAbsoluteOrHome,
  isHomeRelative,
  isPathLike,
  join,
  parent,
  split,
  trimTrailingSeparators,
  type PathStyle,
} from "./hostPath";

describe("isAbsolute", () => {
  it.each<[PathStyle, string, boolean]>([
    ["unix", "/", true],
    ["unix", "/home/me", true],
    ["unix", "//x", true],
    ["unix", "relative/dir", false],
    ["unix", "./dir", false],
    ["unix", "~/dir", false],
    ["unix", "", false],
    ["unix", "C:\\Users\\me", false],
    ["unix", "\\\\server\\share", false],
    ["windows", "C:\\", true],
    ["windows", "C:\\Users\\me", true],
    ["windows", "c:/Users/me", true],
    ["windows", "\\\\server\\share", true],
    ["windows", "\\\\server\\share\\dir", true],
    ["windows", "//server/share/dir", true],
    ["windows", "\\\\?\\C:\\Users\\me", true],
    ["windows", "\\\\?\\UNC\\server\\share\\dir", true],
    ["windows", "\\\\server", false],
    ["windows", "C:", false],
    ["windows", "C:dir", false],
    ["windows", "\\Users\\me", false],
    ["windows", "/home/me", false],
    ["windows", "relative\\dir", false],
    ["windows", "~\\dir", false],
    ["windows", "", false],
  ])("on a %s host, %j is absolute: %s", (style, path, expected) => {
    expect(isAbsolute(path, style)).toBe(expected);
  });
});

describe("split", () => {
  it.each<[PathStyle, string, { root: string; names: string[] }]>([
    ["unix", "/", { root: "/", names: [] }],
    ["unix", "/a/b/", { root: "/", names: ["a", "b"] }],
    ["unix", "/a//b", { root: "/", names: ["a", "b"] }],
    ["unix", "a/b", { root: "", names: ["a", "b"] }],
    ["unix", "/a\\b/c", { root: "/", names: ["a\\b", "c"] }],
    ["unix", "~/a", { root: "", names: ["~", "a"] }],
    ["windows", "C:\\", { root: "C:\\", names: [] }],
    ["windows", "C:\\a\\b\\", { root: "C:\\", names: ["a", "b"] }],
    ["windows", "C:/a\\b/c", { root: "C:/", names: ["a", "b", "c"] }],
    ["windows", "\\\\server\\share", { root: "\\\\server\\share", names: [] }],
    ["windows", "\\\\server\\share\\a\\b", { root: "\\\\server\\share\\", names: ["a", "b"] }],
    ["windows", "\\\\?\\C:\\a", { root: "\\\\?\\C:\\", names: ["a"] }],
    ["windows", "\\\\?\\UNC\\server\\share\\a", { root: "\\\\?\\UNC\\server\\share\\", names: ["a"] }],
    ["windows", "a\\b/c", { root: "", names: ["a", "b", "c"] }],
    ["windows", "~\\a", { root: "", names: ["~", "a"] }],
  ])("on a %s host, %j is %j", (style, path, expected) => {
    expect(split(path, style)).toEqual(expected);
  });
});

describe("basename", () => {
  it.each<[PathStyle, string, string]>([
    ["unix", "/Users/dev/code", "code"],
    ["unix", "/Users/dev/code///", "code"],
    ["unix", "/", "/"],
    ["unix", "//", "/"],
    ["unix", "~/", "~"],
    ["unix", "/a\\b", "a\\b"],
    ["windows", "C:\\Users\\dev\\code", "code"],
    ["windows", "C:/Users/dev/code/", "code"],
    ["windows", "C:\\", "C:\\"],
    ["windows", "\\\\server\\share", "\\\\server\\share"],
    ["windows", "\\\\server\\share\\dir\\", "dir"],
    ["windows", "~\\work", "work"],
  ])("on a %s host, the basename of %j is %j", (style, path, expected) => {
    expect(basename(path, style)).toBe(expected);
  });
});

describe("join", () => {
  it("writes the wire path's names with the separator of a Unix host", () => {
    expect(join("/work/app", "src/a.ts", "unix")).toBe("/work/app/src/a.ts");
    expect(join("/work/app/", "src/a.ts", "unix")).toBe("/work/app/src/a.ts");
    expect(join("/", "src/a.ts", "unix")).toBe("/src/a.ts");
    expect(join("/work/app", "", "unix")).toBe("/work/app");
  });

  it("writes them with backslashes on a Windows host, a drive root and a share root taking no second separator", () => {
    expect(join("C:\\work\\app", "src/a.ts", "windows")).toBe("C:\\work\\app\\src\\a.ts");
    expect(join("C:\\work\\app\\", "src/a.ts", "windows")).toBe("C:\\work\\app\\src\\a.ts");
    expect(join("C:\\", "src/a.ts", "windows")).toBe("C:\\src\\a.ts");
    expect(join("C:/work/app", "src", "windows")).toBe("C:/work/app\\src");
    expect(join("\\\\server\\share\\", "a.ts", "windows")).toBe("\\\\server\\share\\a.ts");
    expect(join("C:\\work\\app", "", "windows")).toBe("C:\\work\\app");
  });
});

describe("parent", () => {
  it.each<[PathStyle, string, string | undefined]>([
    ["unix", "/Users/dev/code", "/Users/dev"],
    ["unix", "/Users/dev/code//", "/Users/dev"],
    ["unix", "/Users", "/"],
    ["unix", "/", undefined],
    ["unix", "//", undefined],
    ["unix", "~/code", "~"],
    ["unix", "~", undefined],
    ["unix", "a/b", "a"],
    ["unix", "a", undefined],
    ["unix", "/a\\b", "/"],
    ["windows", "C:\\Users\\dev", "C:\\Users"],
    ["windows", "C:\\Users\\dev\\", "C:\\Users"],
    ["windows", "C:/Users/dev", "C:/Users"],
    ["windows", "C:\\Users", "C:\\"],
    ["windows", "C:\\", undefined],
    ["windows", "\\\\server\\share\\dir", "\\\\server\\share\\"],
    ["windows", "\\\\server\\share", undefined],
    ["windows", "\\\\server\\share\\", undefined],
    ["windows", "~\\code", "~"],
    ["windows", "a\\b", "a"],
    ["windows", "a", undefined],
  ])("on a %s host, the parent of %j is %j", (style, path, expected) => {
    expect(parent(path, style)).toBe(expected);
  });
});

describe("trimTrailingSeparators", () => {
  it.each<[PathStyle, string, string]>([
    ["unix", "/a/b///", "/a/b"],
    ["unix", "/", "/"],
    ["unix", "a\\", "a\\"],
    ["windows", "C:\\a\\b\\\\", "C:\\a\\b"],
    ["windows", "C:/a/", "C:/a"],
    ["windows", "C:\\", "C:\\"],
    ["windows", "\\\\server\\share\\", "\\\\server\\share\\"],
  ])("on a %s host, %j loses its trailing separators: %j", (style, path, expected) => {
    expect(trimTrailingSeparators(path, style)).toBe(expected);
  });
});

describe("home-relative paths", () => {
  it.each<[PathStyle, string, boolean]>([
    ["unix", "~", true],
    ["unix", "~/work", true],
    ["unix", "~\\work", false],
    ["unix", "~other/dir", false],
    ["windows", "~", true],
    ["windows", "~/work", true],
    ["windows", "~\\work", true],
    ["windows", "~other\\dir", false],
  ])("on a %s host, %j is home-relative: %s", (style, path, expected) => {
    expect(isHomeRelative(path, style)).toBe(expected);
  });

  it.each<[PathStyle, string, string]>([
    ["unix", "~", "/home/dev"],
    ["unix", "~/.claude", "/home/dev/.claude"],
    ["unix", "~other", "~other"],
    ["unix", "a/~", "a/~"],
    ["windows", "~", "C:\\Users\\dev"],
    ["windows", "~\\.claude", "C:\\Users\\dev\\.claude"],
    ["windows", "~/.claude", "C:\\Users\\dev/.claude"],
    ["windows", "~other", "~other"],
  ])("on a %s host, %j expands to %j", (style, path, expected) => {
    expect(expandHome(path, style === "unix" ? "/home/dev" : "C:\\Users\\dev", style)).toBe(expected);
  });
});

describe("isAbsoluteOrHome", () => {
  it.each<[PathStyle, string, boolean]>([
    ["unix", "/home/me/.claude-work", true],
    ["unix", "  /home/me  ", true],
    ["unix", "~/work", true],
    ["unix", "~", true],
    ["unix", "relative/dir", false],
    ["unix", "./dir", false],
    ["unix", "~other/dir", false],
    ["unix", "~\\work", false],
    ["unix", "C:\\work", false],
    ["unix", "", false],
    ["unix", "   ", false],
    ["windows", "C:\\work", true],
    ["windows", "  C:\\work  ", true],
    ["windows", "\\\\server\\share", true],
    ["windows", "~\\work", true],
    ["windows", "~/work", true],
    ["windows", "~", true],
    ["windows", "relative\\dir", false],
    ["windows", "C:work", false],
    ["windows", "/work", false],
    ["windows", "", false],
  ])("on a %s host, %j is absolute or home-relative: %s", (style, path, expected) => {
    expect(isAbsoluteOrHome(path, style)).toBe(expected);
  });
});

describe("display", () => {
  it("writes the home directory itself and anything below it with a leading ~ on a Unix host", () => {
    const paths = { home: "/Users/dev", style: "unix" } as const;
    expect(display("/Users/dev", paths)).toBe("~");
    expect(display("/Users/dev/.claude", paths)).toBe("~/.claude");
    expect(display("/Users/dev/a/b", paths)).toBe("~/a/b");
  });

  it("leaves a path that only shares the home directory's text as a prefix alone", () => {
    const paths = { home: "/Users/dev", style: "unix" } as const;
    expect(display("/Users/dev\\foo", paths)).toBe("/Users/dev\\foo");
    expect(display("/Users/devx", paths)).toBe("/Users/devx");
    expect(display("/Users/devx/code", paths)).toBe("/Users/devx/code");
    expect(display("/opt/tools", paths)).toBe("/opt/tools");
  });

  it("tolerates a trailing separator on the home directory", () => {
    const paths = { home: "/Users/dev/", style: "unix" } as const;
    expect(display("/Users/dev/code", paths)).toBe("~/code");
    expect(display("/Users/dev", paths)).toBe("~");
  });

  it("returns the path unchanged when the home directory is unknown or a root", () => {
    for (const home of [null, undefined, "", "/"]) {
      expect(display("/Users/dev/code", { home, style: "unix" })).toBe("/Users/dev/code");
    }
  });

  it("shows a path in full on a Windows host, whatever the home directory", () => {
    const paths = { home: "C:\\Users\\dev", style: "windows" } as const;
    expect(display("C:\\Users\\dev", paths)).toBe("C:\\Users\\dev");
    expect(display("C:\\Users\\dev\\code", paths)).toBe("C:\\Users\\dev\\code");
    expect(display("\\\\server\\share\\dir", paths)).toBe("\\\\server\\share\\dir");
    expect(display("D:\\work", { home: null, style: "windows" })).toBe("D:\\work");
  });
});

describe("hostPathsOf", () => {
  it("takes the style from the host's facts and is unix before the first snapshot", () => {
    expect(hostPathsOf({ homeDir: "/Users/dev", host: null })).toEqual({ home: "/Users/dev", style: "unix" });
    expect(hostPathsOf({ homeDir: null })).toEqual({ home: null, style: "unix" });
    expect(hostPathsOf({ homeDir: "C:\\Users\\dev", host: { path_style: "windows" } })).toEqual({ home: "C:\\Users\\dev", style: "windows" });
  });
});

describe("isPathLike", () => {
  it.each<[PathStyle, string, boolean, boolean]>([
    ["unix", "/", false, false],
    ["unix", "/a", false, true],
    ["unix", "~/", false, false],
    ["unix", "~/a", false, true],
    ["unix", "src/a.ts", false, true],
    ["unix", "a.ts", false, false],
    ["unix", "a.ts", true, true],
    ["unix", "...", false, false],
    ["unix", "C:\\Users\\a.ts", false, false],
    ["unix", "~\\a", false, false],
    ["unix", "src\\a.ts", false, false],
    ["windows", "C:\\", false, false],
    ["windows", "C:\\a", false, true],
    ["windows", "c:/a/b.ts", false, true],
    ["windows", "\\\\server\\share", false, false],
    ["windows", "\\\\server\\share\\a", false, true],
    ["windows", "~\\", false, false],
    ["windows", "~\\a", false, true],
    ["windows", "~/a", false, true],
    ["windows", "/a", false, true],
    ["windows", "src\\a.ts", false, true],
    ["windows", "src/a.ts", false, true],
    ["windows", "a.ts", false, false],
    ["windows", "a.ts", true, true],
    ["windows", "\\", false, false],
  ])("on a %s host, %j (a line given: %s) is path-like: %s", (style, path, hasLine, expected) => {
    expect(isPathLike(path, hasLine, style)).toBe(expected);
  });
});
