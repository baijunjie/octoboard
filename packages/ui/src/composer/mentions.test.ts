import { describe, expect, it } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { applyMention, mentionAt, mentionPaths, mentionSessions, sessionMention, splitMentionPath } from "./mentions";

describe("mentionAt", () => {
  it("finds an @ at the start of the text or after whitespace, up to the caret", () => {
    expect(mentionAt("@", 1)).toEqual({ start: 0, end: 1, query: "" });
    expect(mentionAt("see @src/ui", 11)).toEqual({ start: 4, end: 11, query: "src/ui" });
    expect(mentionAt("line\n@doc", 9)).toEqual({ start: 5, end: 9, query: "doc" });
  });

  it("runs the mention on to the end of its word past the caret", () => {
    expect(mentionAt("@readme more", 3)).toEqual({ start: 0, end: 7, query: "re" });
  });

  it("finds none for an @ inside a word, after whitespace before the caret, or with a second @", () => {
    expect(mentionAt("mail me@example.com", 19)).toBeUndefined();
    expect(mentionAt("@done and more", 14)).toBeUndefined();
    expect(mentionAt("@a@b", 4)).toBeUndefined();
    expect(mentionAt("no mention", 10)).toBeUndefined();
  });
});

describe("applyMention", () => {
  it("replaces the mention with the inserted text and a space, leaving the caret after it", () => {
    expect(applyMention("see @sr", { start: 4, end: 7, query: "sr" }, "src/app.ts")).toEqual({ text: "see src/app.ts ", caret: 15 });
  });

  it("adds no space where whitespace follows already", () => {
    expect(applyMention("@sr next", { start: 0, end: 3, query: "sr" }, "src")).toEqual({ text: "src next", caret: 3 });
  });
});

it("words a session mention as its title and full id", () => {
  expect(sessionMention("Fix the build", "0b1c-22")).toBe("@「Fix the build」(session 0b1c-22)");
});

describe("mentionSessions", () => {
  const current = sessionOf("me", "c1", "p1", "Me", "idle", { started_at: 50 });
  const title = (session: { title: string }) => session.title;
  const sessions = [
    current,
    sessionOf("other-console", "c2", "p9", "Elsewhere", "idle", { started_at: 90 }),
    sessionOf("console-older", "c1", undefined, "Console older", "idle", { started_at: 10 }),
    sessionOf("same-project", "c1", "p1", "Same project", "working", { started_at: 20 }),
    sessionOf("console-newer", "c1", "p2", "Console newer", "idle", { started_at: 30 }),
    sessionOf("archived", "c1", "p1", "Archived", "archived", { started_at: 99 }),
  ];

  it("offers every other unarchived session: same project, then same console, then other consoles, newest first within each", () => {
    expect(mentionSessions(sessions, current, "", title).map((session) => session.id)).toEqual(["same-project", "console-newer", "console-older", "other-console"]);
  });

  it("keeps the sessions whose title contains the query, ignoring case", () => {
    expect(mentionSessions(sessions, current, "CONSOLE", title).map((session) => session.id)).toEqual(["console-newer", "console-older"]);
  });

  it("puts no session first for a console session, which has no project", () => {
    const consoleSession = sessionOf("cs", "c1", undefined, "Console", "idle");
    expect(mentionSessions([consoleSession, ...sessions.slice(1)], consoleSession, "", title).map((session) => session.id)).toEqual(["console-newer", "same-project", "console-older", "other-console"]);
  });
});

describe("mentionPaths", () => {
  const listed = [
    { path: "README.md", directory: false },
    { path: "src", directory: true },
    { path: "docs", directory: true },
  ];

  it("lists the named directory's matching entries, directories first", () => {
    expect(mentionPaths("", listed, [], 10)).toEqual([
      { path: "src", directory: true },
      { path: "docs", directory: true },
      { path: "README.md", directory: false },
    ]);
  });

  it("adds the files a name search found and the directories above them whose name matches, each once", () => {
    expect(mentionPaths("s", listed, ["src/app/settings.ts", "docs/sessions.md", "src/main.ts"], 10)).toEqual([
      { path: "src", directory: true },
      { path: "docs", directory: true },
      { path: "src/app/settings.ts", directory: false },
      { path: "docs/sessions.md", directory: false },
      { path: "src/main.ts", directory: false },
    ]);
  });

  it("takes only the listing for a query with a slash, and stops at the limit", () => {
    const inSrc = [{ path: "src/app", directory: true }, { path: "src/apple.ts", directory: false }, { path: "src/zoo.ts", directory: false }];
    expect(mentionPaths("src/ap", inSrc, ["ignored.ts"], 10)).toEqual([{ path: "src/app", directory: true }, { path: "src/apple.ts", directory: false }]);
    expect(mentionPaths("src/", inSrc, [], 2)).toHaveLength(2);
  });

  it("splits a typed path into its directory and name parts", () => {
    expect(splitMentionPath("src/ui/Bu")).toEqual({ directory: "src/ui", name: "Bu" });
    expect(splitMentionPath("Bu")).toEqual({ directory: "", name: "Bu" });
  });
});
