import { describe, expect, it } from "vitest";

import { sessionOf } from "./gallery/fixtures/builders";
import { format } from "./i18n/catalog";
import type { Translate } from "./i18n/catalog";
import type { Account } from "./protocol";
import { sessionAgentLabel } from "./sessionLabel";

const t: Translate = ((key: string, ...args: unknown[]) => format("en", key as never, args[0] as never)) as Translate;
const accounts: Account[] = [{ id: "w", agent: "claude", name: "Work", config_dir: "/home/w" }];

describe("sessionAgentLabel", () => {
  it.each([
    ["the default account by its own name, not a path", {}, "Claude Code (Default)"],
    ["a stored account by its name", { account_id: "w", config_dir: "/home/w" }, "Claude Code (Work)"],
    ["a removed account by the directory it recorded", { account_id: "gone", config_dir: "/home/old" }, "Claude Code (/home/old)"],
    ["a removed account that recorded no directory by the agent alone", { account_id: "gone" }, "Claude Code"],
  ] as const)("names %s", (_, extra, expected) => {
    expect(sessionAgentLabel(t, sessionOf("s", "c", "p", "Title", "idle", extra), accounts)).toBe(expected);
  });
});
