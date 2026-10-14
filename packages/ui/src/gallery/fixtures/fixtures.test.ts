// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { GROUPS, SCENARIOS } from ".";
import { CATALOGS, format, messageText, splitPlaceholders, type Message, type MessageKey } from "../../i18n/catalog";
import type { Language } from "../../i18n/languages";
import { en } from "../../i18n/messages/en";
import { createUi, type Matcher, type Ui } from "../interact";
import { changeRowText } from "../../browser/changeRow";
import { changeItems } from "../../browser/changes";
import { SAMPLE_GIT } from "./projectGit";
import { viewerScenarios } from "./viewer";
import { DEFAULT_WIDTH, MIN_HEIGHT } from "../scenario";

/** The arguments of a step that hold a value (a status) and not a name to look for, by position: a
 * value can spell the same as a message, and would be flagged for it. */
const VALUE_ARGUMENTS: Partial<Record<keyof Ui, number[]>> = { hoverChangeStatus: [1] };

/** Every scenario's steps, run against a `Ui` that presses nothing and records the names it was
 * given to look for, in `language`. Only `t`, `session` and `changeRow` are real. `key` and `type`
 * take text, and `hoverCalloutStatus` a title, so they record nothing. */
async function matchersOf(language: Language): Promise<{ id: string; matcher: Matcher }[]> {
  const real = createUi({} as Document, language);
  const matchers: { id: string; matcher: Matcher }[] = [];
  for (const scenario of SCENARIOS) {
    const recording = new Proxy(real, {
      get(target, property: keyof Ui) {
        if (property === "t" || property === "session" || property === "changeRow") return target[property];
        return async (...args: unknown[]) => {
          if (property === "key" || property === "type" || property === "hoverCalloutStatus") return;
          args.forEach((arg, index) => {
            if (VALUE_ARGUMENTS[property]?.includes(index)) return;
            if (typeof arg === "string" || typeof arg === "function") matchers.push({ id: scenario.id, matcher: arg as Matcher });
          });
        };
      },
    });
    for (const step of scenario.steps ?? []) await step(recording);
  }
  return matchers;
}

/** Whether a matcher is English copy that `language`'s catalog translates, so that it finds nothing
 * in a window in `language`. A string is caught when it is a message written out bare, or when it
 * fits a message's literal frame with anything in place of the placeholders: the check does not
 * know English, it knows the shape of the messages, so a string that happens to fit one is caught
 * whatever filled it. A data name that contains `", "`, `" — "` or `" (…)"` (`"notes, drafts.txt"`,
 * `"Ops (EU)"`) fits the frames of `git.row`, `session.withLocation` and `statusItem.section`, and
 * the remedy is to name the control through a helper or a closure. A frame counts only when it has a
 * non-whitespace character of its own, which keeps those generic ones alive. A plural message counts
 * in every form it has. A test of the name is caught only against the bare messages: it is a
 * closure, which says nothing of the values it was built from, so `startsWith("new-name.ts, renamed")`
 * is not caught. */
function englishCopyIn(language: Language): (matcher: Matcher) => boolean {
  const english = (Object.keys(en) as MessageKey[]).flatMap((key) => {
    const message: Message = en[key];
    if (messageText(language, key) === (typeof message === "string" ? message : message.other)) return [];
    return Object.values(typeof message === "string" ? [message] : message).filter((text) => text !== undefined);
  });
  const filled = english.flatMap((text) => {
    const parts = splitPlaceholders(text);
    const literal = parts.filter((_, index) => index % 2 === 0).join("");
    if (parts.length === 1 || literal.trim() === "") return [];
    const escape = (run: string) => run.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    return [new RegExp(`^${parts.map((part, index) => (index % 2 === 0 ? escape(part) : ".+")).join("")}$`)];
  });
  return (matcher) => (typeof matcher === "string" ? english.includes(matcher) || filled.some((form) => form.test(matcher)) : english.some((text) => matcher(text)));
}

describe("gallery scenarios", () => {
  it("have unique ids, which the URL names them by", () => {
    const ids = SCENARIOS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("write no width the gallery already opens at, which would be a scenario carrying nothing", () => {
    expect(SCENARIOS.filter((s) => s.width === DEFAULT_WIDTH).map((s) => s.id)).toEqual([]);
  });

  it("write no height below the shortest window the page takes, which it would not be shown at", () => {
    expect(SCENARIOS.filter((s) => s.height !== undefined && s.height < MIN_HEIGHT).map((s) => s.id)).toEqual([]);
  });

  // A name is looked up in the language the window is in, so a step that names a control with
  // English copy finds nothing in any other language. Only a language with a catalog of its own can
  // show it: the rest render English.
  it.each((Object.keys(CATALOGS) as Language[]).filter((language) => language !== "en"))(
    "name a control by the window's language in %s, never by English copy",
    async (language) => {
      const isEnglishCopy = englishCopyIn(language);
      const found = (await matchersOf(language)).filter(({ matcher }) => isEnglishCopy(matcher));
      expect(found.map(({ id, matcher }) => `${id}: ${String(matcher)}`)).toEqual([]);
    },
  );

  it("tell English copy from a name that is data, as the check above relies on", () => {
    const isEnglishCopy = englishCopyIn("zh-Hans");
    expect(isEnglishCopy("Show whole file")).toBe(true);
    expect(isEnglishCopy("new-name.ts, renamed")).toBe(true);
    expect(isEnglishCopy((name) => name.startsWith("Show whole file"))).toBe(true);
    expect(isEnglishCopy("src")).toBe(false);
    expect(isEnglishCopy("feature/login")).toBe(false);
    expect(isEnglishCopy("new-name.ts，已重命名")).toBe(false);
    expect(isEnglishCopy("Show all 27 unmodified lines")).toBe(true);
    expect(isEnglishCopy("View all (3)")).toBe(true);
    // The limit: a name shaped like a message's frame is flagged, whatever it is. Name such a control
    // through a helper or a closure.
    expect(isEnglishCopy("notes, drafts.txt")).toBe(true);
  });

  it("Git steps match Chinese session and change labels, including rename detail and selection", async () => {
    const t: import("../../i18n/catalog").Translate = (key, ...args) => format("zh-Hans", key, ...args);
    const recorded = (await matchersOf("zh-Hans")).filter(({ id }) => id.startsWith("git-"));
    const sessionMatchers = recorded.filter(({ matcher }) => String(matcher).startsWith("session("));
    expect(sessionMatchers.length).toBeGreaterThan(0);
    for (const { matcher } of sessionMatchers) {
      expect(typeof matcher === "function" && matcher("会话 Fix the summary layout，Claude Code，工作中")).toBe(true);
    }
    const changes = [...Object.values(SAMPLE_GIT.changes), ...Object.values(SAMPLE_GIT.comparisons ?? {})].flatMap((list) => Array.isArray(list) ? list.map(({ entry }) => entry) : []);
    const labels = changeItems(changes).flatMap((item) => [false, true].map((selected) => changeRowText(t, item, selected, false).ariaLabel.replace(/[\u2066-\u2069]/g, "")));
    const rowMatchers = recorded.filter(({ matcher }) => String(matcher).startsWith("changeRow("));
    expect(rowMatchers.length).toBeGreaterThan(0);
    for (const { id, matcher } of rowMatchers) {
      expect(typeof matcher === "function" && labels.some(matcher), `${id}: ${String(matcher)}`).toBe(true);
    }
    const selector = recorded.filter(({ id }) => id === "git-compare").find(({ matcher }) => String(matcher) === "branchOption(main)")!.matcher;
    expect(typeof selector === "function" && selector("main4f2a9c1")).toBe(true);
  });

  it.each(["en", "zh-Hans", "ar"] as const)("viewer change stages follow the window language in %s", async (language) => {
    const subjects = await viewerScenarios.find(({ id }) => id === "viewer-changes")!.viewer!.subjects(language);
    const stages = subjects.flatMap(({ stage }) => stage === undefined ? [] : [stage]);
    expect(new Set(stages)).toEqual(new Set([format(language, "git.group.staged"), format(language, "git.group.unstaged")]));
    expect(subjects.find(({ path, content }) => path === "src/offline.ts" && content.state === "disconnected")!.stage).toBe(format(language, "git.group.unstaged"));
    expect(subjects.filter(({ key }) => key.startsWith("Compare:") || key.startsWith("Untracked:")).every(({ stage }) => stage === undefined)).toBe(true);
  });

  it("leave no group empty, which would list a bare heading", () => {
    const used = new Set(SCENARIOS.map((s) => s.group));
    expect(GROUPS.map((g) => g.title).filter((title) => !used.has(title))).toEqual([]);
  });
});
