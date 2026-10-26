// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { DEFAULT_SCENARIO, GROUPS, SCENARIOS } from ".";
import { CATALOGS, format, messageText, splitPlaceholders, type Message, type MessageKey } from "../../i18n/catalog";
import type { Language } from "../../i18n/languages";
import { en } from "../../i18n/messages/en";
import { createUi, type Matcher, type Ui } from "../interact";
import { changeRowText } from "../../browser/changeRow";
import { changeItems } from "../../browser/changes";
import { listedCommits } from "./changeFixtures";
import { NEVER_PUSHED_GIT, RANKING_TASK, SAMPLE_GIT, UNPUSHED_GIT } from "./projectGit";
import { viewerScenarios } from "./viewer";
import { DEFAULT_WIDTH, MIN_HEIGHT } from "../scenario";

/** The arguments of a step that hold a value (a status) and not a name to look for, by position: a
 * value can spell the same as a message, and would be flagged for it. */
const VALUE_ARGUMENTS: Partial<Record<keyof Ui, number[]>> = { hoverChangeStatus: [1] };

/** Every scenario's steps, run against a `Ui` that presses nothing and records the names it was
 * given to look for, in `language`. Only `t`, `session` and `changeRow` are real. `key` and `type`
 * take text, `hoverCalloutStatus` a title and `daemonEvents` events, so they record nothing. */
async function matchersOf(language: Language): Promise<{ id: string; matcher: Matcher }[]> {
  const real = createUi({} as Document, language);
  const matchers: { id: string; matcher: Matcher }[] = [];
  for (const scenario of SCENARIOS) {
    const recording = new Proxy(real, {
      get(target, property: keyof Ui) {
        if (property === "t" || property === "session" || property === "changeRow") return target[property];
        return async (...args: unknown[]) => {
          if (property === "key" || property === "type" || property === "hoverCalloutStatus" || property === "daemonEvents") return;
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
 * is not caught. Fixture data equal to a bare message is caught too (a project named "Website", once the
 * About section made that a message): the remedy is to rename the data. */
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

/** A short run of punctuation, which is what a separator between two placeholders is. */
const isSeparator = (run: string) => /^[\p{P}\p{S}\s]{1,4}$/u.test(run) && /[\p{P}\p{S}]/u.test(run);

/** The separators the English catalog writes between two placeholders that `language` writes differently,
 * as `[english, language's]` (`", "` and `"，"`, say). Only a run that `isSeparator` accepts counts, which keeps
 * the prose around a placeholder out; one that `language` also writes between placeholders somewhere is
 * dropped, since a matcher accepting it may be taking that message's own form. */
function separatorPairs(language: Language): [string, string][] {
  const formsOf = (message: Message): Record<string, string> => (typeof message === "string" ? { other: message } : { ...message });
  /** The literal runs between two placeholders, and how many parts the text splits into. */
  const betweenPlaceholders = (text: string) => {
    const parts = splitPlaceholders(text);
    return { runs: parts.filter((_, index) => index % 2 === 0 && index > 0 && index < parts.length - 1), size: parts.length };
  };
  const pairs = new Map<string, [string, string]>();
  const own = new Set<string>();
  for (const key of Object.keys(en) as MessageKey[]) {
    const translated = CATALOGS[language]?.[key];
    if (translated === undefined) continue;
    const english = formsOf(en[key]);
    const mine = formsOf(translated);
    for (const text of Object.values(mine)) betweenPlaceholders(text).runs.forEach((run) => own.add(run));
    for (const [category, text] of Object.entries(english)) {
      const other = mine[category];
      if (other === undefined) continue;
      const [a, b] = [betweenPlaceholders(text), betweenPlaceholders(other)];
      if (a.size !== b.size) continue;
      a.runs.forEach((run, index) => {
        if (run !== b.runs[index] && [run, b.runs[index]].every(isSeparator)) pairs.set(`${run}\0${b.runs[index]}`, [run, b.runs[index]]);
      });
    }
  }
  return [...pairs.values()].filter(([english]) => !own.has(english));
}

/** Every string a scenario's state holds, as the names its steps can look for are taken from it. */
function namesIn(scenario: unknown): string[] {
  const names = new Set<string>();
  const seen = new Set<unknown>();
  const walk = (value: unknown, depth: number) => {
    if (typeof value === "string") {
      if (value.trim() !== "" && value.length <= 60) names.add(value);
      return;
    }
    if (typeof value !== "object" || value === null || seen.has(value) || depth > 6) return;
    seen.add(value);
    if (value instanceof Map) value.forEach((item, key) => (walk(key, depth + 1), walk(item, depth + 1)));
    else if (value instanceof Set || Array.isArray(value)) value.forEach((item) => walk(item, depth + 1));
    else Object.values(value).forEach((item) => walk(item, depth + 1));
  };
  walk(scenario, 0);
  return [...names];
}

/** Whether a matcher takes a name followed by the English catalog's separator and not by `language`'s: a
 * test of the name that spells `", "` out finds nothing in a window whose catalog writes `"，"`. Each of
 * `names` is probed with each of `pairs`, the separators the two catalogs write differently
 * (`separatorPairs`), and the matcher is flagged when it takes one with the English separator after it
 * and refuses it with the language's. A string
 * matcher is exact, and a test that ignores what follows the name takes both, so neither is flagged;
 * a test of a different shape of label is out of the probe's reach. */
function spellsEnglishSeparator(pairs: [string, string][], names: string[], matcher: Matcher): boolean {
  if (typeof matcher !== "function") return false;
  return pairs.some(([english, mine]) => names.some((name) => ["", "x"].some((tail) => matcher(`${name}${english}${tail}`) && !matcher(`${name}${mine}${tail}`))));
}

describe("gallery scenarios", () => {
  it("explicitly classifies every scenario under a functional group and defaults to normal", () => {
    expect(DEFAULT_SCENARIO.category).toBe("normal");
    for (const scenario of SCENARIOS) {
      expect(["normal", "exceptional"]).toContain(scenario.category);
      expect(GROUPS.some(({ title }) => title === scenario.group)).toBe(true);
    }
    expect(GROUPS.map(({ title }) => title)).not.toContain("Empty");
    expect(GROUPS.map(({ title }) => title)).not.toContain("Dialogs");
  });

  it("keeps mocked Git warnings out of normal branch badge previews", () => {
    const badges = SCENARIOS.filter(({ group }) => group === "Branch badges");
    for (const scenario of badges.filter(({ category }) => category === "normal")) {
      expect([...scenario.state!.gitStatuses!.values()].filter(({ error }) => error)).toEqual([]);
    }
    for (const id of ["git-badges-errors", "git-badge-focus", "git-badge-focus-long-error"]) {
      const scenario = badges.find((scenario) => scenario.id === id)!;
      expect(scenario.category).toBe("exceptional");
      expect([...scenario.state!.gitStatuses!.values()].some(({ error }) => error)).toBe(true);
    }
  });

  it("keeps failed reads and conflicts out of normal viewer navigation", async () => {
    for (const scenario of viewerScenarios.filter(({ category }) => category === "normal")) {
      const subjects = await scenario.viewer!.subjects("en");
      expect(subjects.length).toBeGreaterThan(0);
      expect(subjects.filter(({ content }) => ["error", "disconnected", "conflict"].includes(content.state))).toEqual([]);
      expect(subjects.some(({ path }) => ["src/strange.brokengrammar", "assets/broken.svg", "src/garbled.ts"].includes(path))).toBe(false);
    }
  });

  it("keeps conflicts and mocked read failures out of normal Git review navigation", () => {
    for (const scenario of SCENARIOS.filter(({ category, git }) => category === "normal" && git)) {
      for (const changes of Object.values(scenario.git!.changes)) {
        expect(Array.isArray(changes)).toBe(true);
        if (Array.isArray(changes)) expect(changes.filter(({ entry, error, expand }) => entry.group === "conflicted" || error || expand?.error || expand?.movedOn)).toEqual([]);
      }
    }
  });

  it("have unique ids, which the URL names them by", () => {
    const ids = SCENARIOS.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("name each live change of a scenario by an id and a label of its own, which the URL and the buttons tell them by", () => {
    for (const scenario of SCENARIOS.filter(({ liveChanges }) => liveChanges)) {
      const changes = scenario.liveChanges!;
      expect(changes.length, scenario.id).toBeGreaterThan(0);
      expect(new Set(changes.map(({ id }) => id)).size, scenario.id).toBe(changes.length);
      // The frame URL's `live` parameter lists them comma-separated.
      expect(changes.filter(({ id }) => id.includes(",")).map(({ id }) => id), scenario.id).toEqual([]);
      expect(new Set(changes.map(({ label }) => label)).size, scenario.id).toBe(changes.length);
    }
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

  it.each((Object.keys(CATALOGS) as Language[]).filter((language) => language !== "en"))(
    "name a control by a name followed by the window's own separator in %s, never the English one",
    async (language) => {
      const pairs = separatorPairs(language);
      const names = new Map(SCENARIOS.map((scenario) => [scenario.id, namesIn(scenario)]));
      const found = (await matchersOf(language)).filter(({ id, matcher }) => spellsEnglishSeparator(pairs, names.get(id)!, matcher));
      expect(found.map(({ id, matcher }) => `${id}: ${String(matcher)}`)).toEqual([]);
    },
  );

  it("tell a matcher that spells the English separator out from one that takes any punctuation, as the check above relies on", () => {
    const zhPairs = separatorPairs("zh-Hans");
    expect(zhPairs).toContainEqual([", ", "，"]);
    const spelled = (label: string) => label === "Operations" || label.startsWith("Operations, ");
    const anyPunctuation = (label: string) => label === "Operations" || (label.startsWith("Operations") && /^\p{P}/u.test(label.slice("Operations".length)));
    expect(spellsEnglishSeparator(zhPairs, ["Operations"], spelled)).toBe(true);
    expect(spellsEnglishSeparator(zhPairs, ["Operations"], anyPunctuation)).toBe(false);
    expect(spellsEnglishSeparator(zhPairs, ["Operations"], "Operations, x")).toBe(false);
    expect(spellsEnglishSeparator(zhPairs, ["Operations"], (label) => label === "Operations")).toBe(false);
  });

  it("tell English copy from a name that is data, as the check above relies on", () => {
    const isEnglishCopy = englishCopyIn("zh-Hans");
    expect(isEnglishCopy("Show whole file")).toBe(true);
    expect(isEnglishCopy("new-name.ts, renamed")).toBe(true);
    expect(isEnglishCopy((name) => name.startsWith("Show whole file"))).toBe(true);
    expect(isEnglishCopy("src")).toBe(false);
    expect(isEnglishCopy("feature/login")).toBe(false);
    expect(isEnglishCopy(format("zh-Hans", "git.row", { name: "new-name.ts", status: format("zh-Hans", "git.status.renamed") }))).toBe(false);
    expect(isEnglishCopy("Show all 27 unmodified lines")).toBe(true);
    expect(isEnglishCopy("View all")).toBe(true);
    // The limit: a name shaped like a message's frame is flagged, whatever it is. Name such a control
    // through a helper or a closure.
    expect(isEnglishCopy("notes, drafts.txt")).toBe(true);
  });

  it("Git steps match Chinese session and change labels, including rename detail and selection", async () => {
    const t: import("../../i18n/catalog").Translate = (key, ...args) => format("zh-Hans", key, ...args);
    const recorded = (await matchersOf("zh-Hans")).filter(({ id }) => id.startsWith("git-"));
    const sessionMatchers = recorded.filter(({ matcher }) => String(matcher).startsWith("session("));
    expect(sessionMatchers.length).toBeGreaterThan(0);
    const sessionLabels = [
      t("sidebar.session.ariaLabel", { title: "Fix the summary layout", agent: "Claude Code", status: t("session.status.working") }),
      t("sidebar.session.ariaLabel", { title: "Tune the ranking weights", agent: "Claude Code", status: t("session.status.idle") }),
      t("sidebar.session.ariaLabel", { title: "Update the dependencies", agent: "Claude Code", status: t("session.status.idle") }),
      // A session with a task is named by both, as the scenario for the branch badges selects it.
      t("sidebar.session.ariaLabel", {
        title: t("session.displayTitle", { title: RANKING_TASK, name: "Tune the ranking weights" }),
        agent: "Claude Code",
        status: t("session.status.idle"),
      }),
    ];
    for (const { matcher } of sessionMatchers) {
      expect(typeof matcher === "function" && sessionLabels.some(matcher)).toBe(true);
    }
    const repositories = [SAMPLE_GIT, UNPUSHED_GIT, NEVER_PUSHED_GIT];
    const commits = repositories.flatMap((git) => Object.values(git.commits ?? {}).flatMap((list) => listedCommits(list).map(({ changes: made }) => made)));
    const changes = [...Object.values(SAMPLE_GIT.changes), ...repositories.flatMap((git) => Object.values(git.comparisons ?? {})), ...commits].flatMap((list) => Array.isArray(list) ? list.map(({ entry }) => entry) : []);
    const labels = changeItems(changes).flatMap((item) => [false, true].map((selected) => changeRowText(t, item, selected, false).ariaLabel.replace(/[\u2066-\u2069]/g, "")));
    const rowMatchers = recorded.filter(({ matcher }) => String(matcher).startsWith("changeRow("));
    expect(rowMatchers.length).toBeGreaterThan(0);
    for (const { id, matcher } of rowMatchers) {
      expect(typeof matcher === "function" && labels.some(matcher), `${id}: ${String(matcher)}`).toBe(true);
    }
  });

  it.each(["en", "zh-Hans", "ar"] as const)("viewer change stages follow the window language in %s", async (language) => {
    const subjects = [...await viewerScenarios.find(({ id }) => id === "viewer-changes")!.viewer!.subjects(language), ...await viewerScenarios.find(({ id }) => id === "viewer-changes-errors")!.viewer!.subjects(language)];
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
