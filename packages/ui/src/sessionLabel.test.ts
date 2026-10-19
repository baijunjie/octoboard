import { describe, expect, it } from "vitest";

import { sessionOf } from "./gallery/fixtures/builders";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import type { Account } from "./protocol";
import { sessionAgentLabel, sessionAriaLabel, sessionDisplayTitle, sessionStatusKind, sessionWithLocation } from "./sessionLabel";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const accounts: Account[] = [{ id: "w", agent: "claude", name: "Work", config_dir: "/home/w" }];

describe("sessionAgentLabel", () => {
  it.each([
    ["the default account by its own name, not a path", {}, `Claude Code (${isolate("Default")})`],
    ["a stored account by its name", { account_id: "w", config_dir: "/home/w" }, `Claude Code (${isolate("Work")})`],
    ["a removed account by the directory it recorded", { account_id: "gone", config_dir: "/home/old" }, `Claude Code (${isolate("/home/old")})`],
    ["a removed account that recorded no directory by the agent alone", { account_id: "gone" }, "Claude Code"],
  ] as const)("names %s", (_, extra, expected) => {
    expect(sessionAgentLabel(t, sessionOf("s", "c", "p", "Title", "idle", extra), accounts, undefined)).toBe(expected);
  });
});


describe("sessionStatusKind", () => {
  it("reads idle with work left running as running in the background, and nothing else", () => {
    const tasks = [{ id: "b1" }];
    expect(sessionStatusKind(sessionOf("s", "c", "p", "T", "idle", { background_tasks: tasks }))).toBe("background");
    expect(sessionStatusKind(sessionOf("s", "c", "p", "T", "idle", { background_tasks: [] }))).toBe("idle");
    expect(sessionStatusKind(sessionOf("s", "c", "p", "T", "working", { background_tasks: tasks }))).toBe("working");
  });

  it("puts the name into the row's accessible name", () => {
    const session = sessionOf("s", "c", "p", "T", "idle", { background_tasks: [{ id: "b1" }] });
    expect(sessionAriaLabel(t, "en", session, accounts, undefined, undefined)).toContain("Running in background");
  });
});

describe("sessionDisplayTitle", () => {
  it.each([undefined, null, "", " \n ", "octoboard", " octoboard "])("keeps the created name for an absent or identical title: %s", (terminal_title) => {
    const session = sessionOf("s", "c", "p", "octoboard", "idle", { terminal_title });
    expect(sessionDisplayTitle(t, session)).toBe("octoboard");
  });

  it("combines trimmed task and created names with independent reading directions", () => {
    const session = sessionOf("s", "c", "p", "octoboard", "working", { terminal_title: "  شرح المعرض  " });
    const title = `${isolate("شرح المعرض")}｜${isolate("octoboard")}`;
    expect(sessionDisplayTitle(t, session)).toBe(title);
    expect(sessionAriaLabel(t, "en", session, [], undefined, undefined)).toContain(isolate(title));
    expect(sessionWithLocation(t, session, new Map(), new Map())).toContain(isolate(title));
    const zh: Translate = ((key: string, ...args: unknown[]) => format("zh-Hans", key as never, args[0] as never)) as Translate;
    expect(sessionDisplayTitle(zh, session)).toBe(title);
    expect(session.title).toBe("octoboard");
  });
});
