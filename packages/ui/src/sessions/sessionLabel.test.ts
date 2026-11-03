import { describe, expect, it } from "vitest";

import { accountOf, projectOf, sessionOf } from "../fixtures/builders";
import { format } from "../i18n/catalog";
import { displayColumns } from "../i18n/displayColumns";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Account } from "../protocol";
import { sessionChoiceTitle, sessionAgentLabel, sessionAriaLabel, sessionDisplayTitle, sessionRowName, sessionWithLocation, statusLabel } from "./sessionLabel";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const accounts: Account[] = [accountOf("w", "claude", "Work", "/home/w")];

describe("sessionAgentLabel", () => {
  it.each([
    ["the default account by its own name, not a path", {}, `Claude Code (${isolate("Default")})`],
    ["a stored account by its name", { account_id: "w", config_dir: "/home/w" }, `Claude Code (${isolate("Work")})`],
    ["a removed account by the directory it recorded", { account_id: "gone", config_dir: "/home/old" }, `Claude Code (${isolate("/home/old")})`],
    ["a removed account that recorded no directory by the agent alone", { account_id: "gone" }, "Claude Code"],
  ] as const)("names %s", (_, extra, expected) => {
    expect(sessionAgentLabel(t, sessionOf("s", "c", "p", "Title", "idle", extra), accounts, {})).toBe(expected);
  });
});


describe("sessionAriaLabel", () => {
  it.each([
    ["background", "Working in background"],
    ["done", "Done"],
    ["delegated", "Child sessions working"],
    ["paused", "Paused"],
    ["dormant", "Dormant"],
  ] as const)("puts the %s status into the row's accessible name", (status, text) => {
    const session = sessionOf("s", "c", "p", "T", "idle");
    expect(sessionAriaLabel(t, "en", session, accounts, undefined, {}, status)).toContain(text);
  });
});

describe("statusLabel", () => {
  it.each([
    ["dormant", "en", "Dormant"],
    ["paused", "en", "Paused"],
    ["dormant", "zh-Hans", "已休眠"],
    ["paused", "zh-Hans", "已暂停"],
  ] as const)("words %s in %s as just that, with no reason", (status, language, text) => {
    const translate = ((key: string, ...args: unknown[]) => format(language, key as never, args[0] as never)) as Translate;
    expect(statusLabel(translate, status)).toBe(text);
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
    expect(sessionAriaLabel(t, "en", session, [], undefined, {}, "idle")).toContain(isolate(title));
    expect(sessionWithLocation(t, session, new Map(), new Map())).toContain(isolate(title));
    const zh: Translate = ((key: string, ...args: unknown[]) => format("zh-Hans", key as never, args[0] as never)) as Translate;
    expect(sessionDisplayTitle(zh, session)).toBe(title);
    expect(session.title).toBe("octoboard");
  });
});

describe("sessionWithLocation with a length cap", () => {
  const projectMap = new Map([["p", projectOf("p", "c", "Alpha")]]);

  it("shortens a long title with an ellipsis and keeps the location", () => {
    const session = sessionOf("s", "c", "p", "word ".repeat(40), "idle");
    const line = sessionWithLocation(t, session, new Map(), projectMap, 60);
    expect(displayColumns(line)).toBeLessThanOrEqual(60);
    expect(line).toContain("…");
    expect(line.endsWith(` — ${isolate("Alpha")}`)).toBe(true);
  });

  it("closes an isolate the cut leaves open, and cuts a long location too", () => {
    const session = sessionOf("s", "c", "p", "octoboard", "working", { terminal_title: "x".repeat(100) });
    const line = sessionWithLocation(t, session, new Map(), new Map([["p", projectOf("p", "c", "L".repeat(80))]]), 60);
    expect(line.match(/\u2068/g)?.length).toBe(line.match(/\u2069/g)?.length);
    expect(displayColumns(line)).toBeLessThanOrEqual(60);
  });

  it("leaves a line that fits whole", () => {
    const session = sessionOf("s", "c", "p", "Fix bug", "idle");
    expect(sessionWithLocation(t, session, new Map(), projectMap, 60)).toBe(sessionWithLocation(t, session, new Map(), projectMap));
  });
});

it("keeps composed session choice labels raw while sentence display keeps each name isolated", () => {
  const session = sessionOf("s", "c", "p", "مراجعة Work", "idle", { terminal_title: "Task 中文" });
  expect(sessionChoiceTitle(t, session)).toBe("Task 中文｜مراجعة Work");
  expect(sessionDisplayTitle(t, session)).toBe(`${isolate("Task 中文")}｜${isolate("مراجعة Work")}`);
});

describe("sessionRowName", () => {
  const session = sessionOf("s", "c", "p", "Work", "idle");
  const coordinator = sessionOf("coordinator", "c", "p", "Coordinator", "idle");
  const hub = sessionOf("hub", "c", undefined, "Hub", "idle", { role: "console" });
  const name = (owners: (typeof session | undefined)[]) => sessionRowName(t, "en", session, { accounts, defaultNames: {}, owners, modelLabel: "Opus 5.5 · high", status: "idle" });
  const base = sessionAriaLabel(t, "en", session, accounts, undefined, {}, "idle");

  it("names no owner, one owner or two, with the model line last", () => {
    expect(name([])).toBe(`${base}, Opus 5.5 · high`);
    expect(name([undefined, undefined])).toBe(name([]));
    expect(name([coordinator])).toBe(`${base}, Bound to ${isolate("Coordinator")}, Opus 5.5 · high`);
    expect(name([coordinator, hub])).toBe(`${base}, Bound to ${isolate("Coordinator")}, Bound to ${isolate("Hub")}, Opus 5.5 · high`);
  });
});
