import { describe, expect, it } from "vitest";

import { REVIEW } from "../fixtures/configurationReview";
import type { ImportedConsole, InspectedAccount, InspectedAccounts, InspectedConsole } from "../protocol";
import { defaultBinding } from "../shortcuts/commands";
import { BROWSER_HOST } from "../test/hosts";
import { TEST_PROVIDERS } from "../test/testProviders";
import {
  accountChoicesFor,
  accountResolutionsOf,
  agentImported,
  accountsNeedAttention,
  applyToAll,
  availableFor,
  cardAgents,
  carriedParts,
  chosen,
  choicesFor,
  directoryProblem,
  documentShortcuts,
  hasAccountDirectoryProblem,
  importedAgents,
  importedParts,
  importedSections,
  initialAccountReview,
  initialReview,
  outcomeOf,
  regionsOf,
  resolutionsOf,
  reviewForOffers,
  nothingToImport,
  shortcutsRecord,
  shortcutsReview as shortcutsReviewFor,
  takesDocumentCloneDir,
  targetOf,
  withAgentImported,
  withEffectiveFallbacks,
  withRow,
} from "./importPlan";

const shortcutsReview = (shortcuts: Record<string, unknown>, current: Parameters<typeof shortcutsReviewFor>[1]) => shortcutsReviewFor(shortcuts, current, BROWSER_HOST);

const consoles = REVIEW.consoles;
const [website, searchApi, designSystem, notes] = consoles[0].projects;

it("offers a project only the choices its state allows, and pointing at a directory or leaving it out always", () => {
  expect(choicesFor(website)).toEqual(["exported_path", "default_clone_dir", "directory", "skip"]);
  expect(choicesFor(searchApi)).toEqual(["default_clone_dir", "directory", "skip"]);
  expect(choicesFor(designSystem)).toEqual(["clone", "directory", "skip"]);
  expect(choicesFor(notes)).toEqual(["directory", "skip"]);
});

it("starts every project on the daemon's default, and leaves out one that has none", () => {
  expect(initialReview(consoles).map((rows) => rows.map((row) => row.choice))).toEqual([
    ["exported_path", "default_clone_dir", "clone", "skip"],
    ["clone"],
    [],
  ]);
});

it("applies a choice to the projects that offer it and leaves every other row as it was", () => {
  const start = withRow(initialReview(consoles), 0, 0, { choice: "directory", directory: "~/web" });
  const cloned = applyToAll(consoles, start, "clone");
  expect(cloned.map((rows) => rows.map((row) => row.choice))).toEqual([
    ["directory", "default_clone_dir", "clone", "skip"],
    ["clone"],
    [],
  ]);
  // The typed directory is kept under a choice that is not the directory, so going back finds it.
  expect(applyToAll(consoles, start, "default_clone_dir")[0][0]).toEqual({ choice: "default_clone_dir", directory: "~/web", chosen: true });
});

it("leaves out every project when asked to, whatever it offers", () => {
  const left = applyToAll(consoles, initialReview(consoles), "skip");
  expect(left.flat().map((row) => row.choice)).toEqual(["skip", "skip", "skip", "skip", "skip"]);
});

it("counts the projects a choice is available for", () => {
  expect(
    (["exported_path", "default_clone_dir", "clone", "skip"] as const).map((choice) => availableFor(consoles, choice)),
  ).toEqual([1, 2, 2, 5]);
});

describe("reviewForOffers", () => {
  const offers = (clone: string | null) => ({ exported_path: null, default_clone_dir: null, clone });
  const projectOf = (now: InspectedConsole["projects"][number]["default"], offered: string | null) => ({ ...website, offers: offers(offered), default: now });
  const after = [{ ...consoles[0], projects: [projectOf("clone", "/c/a"), projectOf(null, null), projectOf("clone", "/c/b")] }];
  const review = [[{ choice: "skip" as const, directory: "" }, { choice: "clone" as const, directory: "" }, { choice: "skip" as const, directory: "" }]];

  it("moves a row the user never touched to the new preselected choice, or leaves it out where there is none", () => {
    expect(reviewForOffers(review, after)[0].map((row) => row.choice)).toEqual(["clone", "skip", "clone"]);
  });

  it("keeps the choice of a row the user chose for, even a skip, unless its clone-dependent target is gone", () => {
    const chosen = withRow(withRow(withRow(review, 0, 0, review[0][0]), 0, 1, review[0][1]), 0, 2, review[0][2]);
    expect(reviewForOffers(chosen, after)[0].map((row) => row.choice)).toEqual(["skip", "skip", "skip"]);
  });
});

it("replaces one row and keeps every other the same object", () => {
  const start = initialReview(consoles);
  const next = withRow(start, 0, 1, { choice: "skip", directory: "" });
  expect(next[0][1]).toEqual({ choice: "skip", directory: "", chosen: true });
  expect(next[0][0]).toBe(start[0][0]);
  expect(next[1]).toBe(start[1]);
});

it.each([
  { directory: "", problem: "empty" },
  { directory: "relative/dir", problem: "not_absolute" },
  { directory: "~other/dir", problem: "not_absolute" },
  { directory: "/srv/web", problem: undefined },
  { directory: "~/code/web", problem: undefined },
])("judges the directory typed for a project: $directory", ({ directory, problem }) => {
  expect(directoryProblem({ choice: "directory", directory })).toBe(problem);
});

it.each([
  { directory: "relative\\dir", problem: "not_absolute" },
  { directory: "/srv/web", problem: "not_absolute" },
  { directory: "C:web", problem: "not_absolute" },
  { directory: "~", problem: "not_absolute" },
  { directory: "C:\\web", problem: undefined },
  { directory: "\\\\server\\share\\web", problem: undefined },
  { directory: "~\\code\\web", problem: undefined },
  { directory: "~/code/web", problem: undefined },
])("judges the directory typed for a project on a Windows host: $directory", ({ directory, problem }) => {
  expect(directoryProblem({ choice: "directory", directory }, "windows")).toBe(problem);
});

it("takes a Windows path as no directory on a Unix host", () => {
  expect(directoryProblem({ choice: "directory", directory: "C:\\web" })).toBe("not_absolute");
});

it("finds no fault with the directory of a row that does not use one", () => {
  expect(directoryProblem({ choice: "skip", directory: "relative/dir" })).toBeUndefined();
  expect(directoryProblem({ choice: "clone", directory: "" })).toBeUndefined();
});

it("shows the directory each choice would use", () => {
  expect(targetOf(website, { choice: "exported_path", directory: "" })).toBe("/Users/ana/code/website");
  expect(targetOf(website, { choice: "default_clone_dir", directory: "" })).toBe("/Users/dev/Projects/website");
  expect(targetOf(designSystem, { choice: "clone", directory: "" })).toBe("/Users/dev/Projects/design-system");
  expect(targetOf(notes, { choice: "directory", directory: "~/notes" })).toBe("~/notes");
  expect(targetOf(notes, { choice: "directory", directory: "" })).toBeUndefined();
  expect(targetOf(notes, { choice: "skip", directory: "" })).toBeUndefined();
});

it("sends one resolution for each project that is placed, by its position in the document, and none for one left out", () => {
  const review = withRow(initialReview(consoles), 0, 3, { choice: "directory", directory: "~/notes" });
  expect(resolutionsOf(review)).toEqual([
    { console: 0, project: 0, choice: { kind: "exported_path" } },
    { console: 0, project: 1, choice: { kind: "default_clone_dir" } },
    { console: 0, project: 2, choice: { kind: "clone" } },
    { console: 0, project: 3, choice: { kind: "directory", path: "~/notes" } },
    { console: 1, project: 0, choice: { kind: "clone" } },
  ]);
  expect(resolutionsOf(applyToAll(consoles, review, "skip"))).toEqual([]);
});

describe("outcomeOf", () => {
  const project = (name: string, status: ImportedConsole["projects"][number]["status"], fallbacks = 0) => ({
    name,
    status,
    via: null,
    project: null,
    error: null,
    account_fallbacks: Array.from({ length: fallbacks }, () => ({ agent: "codex" as const, name: "Personal" })),
  });
  const imported: ImportedConsole[] = [
    {
      name: "A",
      status: "created",
      console: "c1",
      error: null,
      account_fallbacks: [{ agent: "claude", name: "Work" }],
      projects: [project("a1", "created", 1), project("a2", "left_out"), project("a3", "failed")],
    },
    { name: "B", status: "failed", console: null, error: null, account_fallbacks: [], projects: [project("b1", "not_attempted")] },
  ];

  it("counts what was created, left out, failed and not tried", () => {
    expect(outcomeOf(imported)).toMatchObject({
      consolesCreated: 1,
      consolesFailed: 1,
      projectsCreated: 1,
      projectsLeftOut: 1,
      projectsFailed: 1,
      projectsNotAttempted: 1,
    });
  });
});

describe("the keyboard shortcuts of a document", () => {
  const focusMode = { code: "KeyK", primary: true, shift: true };
  const quickCreate = { code: "KeyJ", primary: true, shift: true };

  it("are the object in its settings, an empty one meaning every default, and nothing for anything else", () => {
    expect(documentShortcuts({ settings: { keyboard_shortcuts: { focusMode } } })).toEqual({ focusMode });
    expect(documentShortcuts({ settings: { keyboard_shortcuts: {} } })).toEqual({});
    for (const document of [undefined, null, {}, { settings: null }, { settings: {} }, { settings: { keyboard_shortcuts: null } }, { settings: { keyboard_shortcuts: [focusMode] } }, { settings: { keyboard_shortcuts: "focusMode" } }]) {
      expect(documentShortcuts(document)).toBeUndefined();
    }
    expect(shortcutsRecord(7)).toBeUndefined();
  });

  it("are counted as those that can stand, and compared with the ones given", () => {
    expect(shortcutsReview({ focusMode, bad: 1 }, {})).toMatchObject({ count: 1, changed: [expect.anything()] });
    expect(shortcutsReview({ focusMode }, { focusMode })).toMatchObject({ count: 1, changed: [] });
    expect(shortcutsReview({}, {})).toMatchObject({ count: 0, changed: [] });
    expect(shortcutsReview({}, { quickCreate })).toMatchObject({ count: 0, changed: [expect.anything()] });
  });

  it("list the commands whose binding would change, with the binding they would get and the one they have", () => {
    const focusModeDefault = defaultBinding("focusMode");
    const quickCreateDefault = defaultBinding("quickCreate");
    expect(shortcutsReview({ focusMode, bad: 1 }, {}).changed).toEqual([{ label: "settings.shortcuts.command.focusMode", binding: focusMode, current: focusModeDefault }]);
    expect(shortcutsReview({ focusMode }, { focusMode }).changed).toEqual([]);
    expect(shortcutsReview({}, { quickCreate }).changed).toEqual([{ label: "settings.shortcuts.command.quickCreate", binding: quickCreateDefault, current: quickCreate }]);
    // Only the commands that differ, in the order the commands are listed.
    expect(shortcutsReview({ quickCreate: focusMode }, { focusMode }).changed).toEqual([
      { label: "settings.shortcuts.command.focusMode", binding: focusModeDefault, current: focusMode },
      { label: "settings.shortcuts.command.quickCreate", binding: focusMode, current: quickCreateDefault },
    ]);
  });

  it("count a swap and a chain whole", () => {
    expect(shortcutsReview({ focusMode: { code: "KeyN", primary: true, shift: true }, quickCreate: { code: "KeyF", primary: true, shift: true } }, {})).toMatchObject({ count: 2, changed: [expect.anything(), expect.anything()] });
  });
});

describe("cardAgents", () => {
  it("lists the agents given first, then the agents the settings or tags name, those the daemon serves only", () => {
    expect(cardAgents(TEST_PROVIDERS, undefined, ["claude"])).toEqual(["claude"]);
    expect(cardAgents(TEST_PROVIDERS, { provider_settings: { codex: { claude_compatibility: false } }, agent_tags: { codex: ["a"], grok: [] } }, ["grok"])).toEqual(["grok", "codex"]);
    expect(cardAgents(TEST_PROVIDERS, { provider_settings: {}, agent_tags: { someone_new: ["x"], constructor: ["x"], claude: [] } as never }, [])).toEqual(["claude"]);
    expect(cardAgents(TEST_PROVIDERS, { provider_settings: {}, agent_tags: { claude: ["a"] } }, ["grok"], ["codex", "grok"])).toEqual(["grok", "codex", "claude"]);
  });
});

describe("accounts", () => {
  const account = (name: string, status: InspectedAccount["status"]): InspectedAccount => ({
    name,
    directory: `~/.${name}`,
    foreign_directory: false,
    proposed_directory: `/home/.${name}`,
    status,
    in_use_by: null,
    error: null,
    disabled: false,
  });
  const accounts: InspectedAccounts = {
    agents_to_add: [],
    agents: [
      { agent: "claude", default_name: null, default_name_applies: false, default_disabled: null, default_disabled_applies: false, accounts: [account("a", "new"), account("b", "existing"), account("c", "directory_in_use"), account("d", "invalid")] },
      { agent: "codex", default_name: null, default_name_applies: false, default_disabled: null, default_disabled_applies: false, accounts: [account("e", "new")] },
    ],
    agent_settings: null,
  };

  it("creates what can be created as proposed and leaves out what cannot", () => {
    expect(initialAccountReview(accounts).rows.map((rows) => rows.map((row) => row.choice))).toEqual([["create", "create", "leave_out", "leave_out"], ["create"]]);
    expect(initialAccountReview(accounts).leftOut).toEqual([]);
    expect(initialAccountReview(undefined)).toEqual({ rows: [], leftOut: [] });
  });

  it("sends a resolution only for an account moved or left out, named by its index among its agent's accounts, never for an existing one", () => {
    const review = initialAccountReview(accounts);
    review.rows[0][0] = { choice: "leave_out", directory: "" };
    review.rows[0][1] = { choice: "leave_out", directory: "" };
    review.rows[0][3] = { choice: "directory", directory: "~/moved" };
    expect(accountResolutionsOf(accounts, review)).toEqual([
      { agent: "claude", account: 0, leave_out: true },
      { agent: "claude", account: 2, leave_out: true },
      { agent: "claude", account: 3, directory: "~/moved" },
    ]);
  });

  it("finds a typed account directory the daemon would refuse only on an account placed in a directory", () => {
    const review = initialAccountReview(accounts);
    expect(hasAccountDirectoryProblem(accounts, review)).toBe(false);
    review.rows[0][2] = { choice: "directory", directory: "" };
    expect(hasAccountDirectoryProblem(accounts, review)).toBe(true);
    review.rows[0][2] = { choice: "directory", directory: "relative" };
    expect(hasAccountDirectoryProblem(accounts, review)).toBe(true);
    review.rows[0][2] = { choice: "directory", directory: "~/x" };
    expect(hasAccountDirectoryProblem(accounts, review)).toBe(false);
  });

  it("sends nothing for an agent left out and ignores a directory typed for its accounts", () => {
    const review = withAgentImported(initialAccountReview(accounts), "claude", false);
    review.rows[0][0] = { choice: "leave_out", directory: "" };
    review.rows[0][2] = { choice: "directory", directory: "" };
    expect(agentImported(review, "claude")).toBe(false);
    expect(agentImported(review, "codex")).toBe(true);
    expect(accountResolutionsOf(accounts, review)).toEqual([]);
    expect(hasAccountDirectoryProblem(accounts, review)).toBe(false);
    const back = withAgentImported(review, "claude", true);
    expect(back.leftOut).toEqual([]);
    expect(back.rows).toBe(review.rows);
    expect(accountResolutionsOf(accounts, back)).toHaveLength(3);
  });

  it("applies the agents of its cards that are checked", () => {
    const withSettings = { ...accounts, agents_to_add: ["grok" as const], agent_settings: { provider_settings: { codex: {} }, agent_tags: { claude: ["x"] } } as never };
    const review = initialAccountReview(withSettings);
    expect(importedAgents(TEST_PROVIDERS, withSettings, review)).toEqual(["claude", "codex", "grok"]);
    expect(importedAgents(TEST_PROVIDERS, withSettings, withAgentImported(review, "codex", false))).toEqual(["claude", "grok"]);
  });

  it("asks for attention for a directory in use or an invalid account, not for a new or existing one", () => {
    const review = initialAccountReview(accounts);
    expect(accountsNeedAttention(accounts, review)).toBe(true);
    const settled = { ...accounts, agents: [{ ...accounts.agents[1] }, { ...accounts.agents[0], accounts: [account("a", "new"), account("b", "existing")] }] };
    expect(accountsNeedAttention(settled, initialAccountReview(settled))).toBe(false);
  });

  it("does not ask for attention for an account of an agent left out", () => {
    const review = initialAccountReview(accounts);
    expect(accountsNeedAttention(accounts, withAgentImported(review, "claude", false))).toBe(false);
    expect(accountsNeedAttention(accounts, withAgentImported(withAgentImported(review, "claude", false), "claude", true))).toBe(true);
  });

  describe("a reference to an account of the document", () => {
    const reference = (name: string, agent: "claude" | "codex" = "claude") => ({ agent, name });
    const references = [reference("A"), reference("c"), reference("D"), reference("gone"), reference("E", "codex")];
    const referencing: InspectedConsole[] = [
      { name: "Hub", account_fallbacks: references, projects: [{ ...website, account_fallbacks: references }] },
    ];
    const fallenBack = (review: ReturnType<typeof initialAccountReview>, importing = true) => {
      const [hub] = withEffectiveFallbacks(referencing, accounts, review, importing);
      expect(hub.projects[0].account_fallbacks).toEqual(hub.account_fallbacks);
      return hub.account_fallbacks.map((fallback) => fallback.name);
    };

    it("binds to an account the import creates, however it is created, and falls back for one it leaves out", () => {
      const review = initialAccountReview(accounts);
      // A is new and created, and E too; C and D cannot be created as proposed and start left out.
      expect(fallenBack(review)).toEqual(["c", "D", "gone"]);
      review.rows[0][2] = { choice: "directory", directory: "~/c" };
      review.rows[0][3] = { choice: "directory", directory: "~/d" };
      expect(fallenBack(review)).toEqual(["gone"]);
      review.rows[0][0] = { choice: "leave_out", directory: "" };
      review.rows[1][0] = { choice: "leave_out", directory: "" };
      expect(fallenBack(review)).toEqual(["A", "gone", "E"]);
    });

    it("falls back for a new account of an agent left out, as for one left out, but not for one that exists already", () => {
      const review = withAgentImported(initialAccountReview(accounts), "claude", false);
      // A is new and E is of the agent still imported; B exists and is bound whatever the agent's choice.
      expect(fallenBack(review)).toEqual(["A", "c", "D", "gone"]);
      const withExisting = [{ ...referencing[0], account_fallbacks: [reference("B")] }];
      expect(withEffectiveFallbacks(withExisting, accounts, review, true)[0].account_fallbacks).toEqual([]);
    });

    it("keeps the fallback for an account that cannot be created for its name, which no directory cures", () => {
      const taken = { ...account("A", "invalid"), error: { code: "account_name_taken", params: {}, message: "taken" } };
      const named = { ...accounts, agents: [{ ...accounts.agents[0], accounts: [taken] }] };
      expect(accountChoicesFor(taken)).toEqual(["leave_out"]);
      expect(accountChoicesFor(account("A", "invalid"))).toEqual(["directory", "leave_out"]);
      const blankName = { ...account("A", "invalid"), error: { code: "field_required", params: { field: "name" }, message: "blank" } };
      expect(accountChoicesFor(blankName)).toEqual(["leave_out"]);
      const blankDirectory = { ...account("A", "invalid"), error: { code: "field_required", params: { field: "config_dir" }, message: "blank" } };
      expect(accountChoicesFor(blankDirectory)).toEqual(["directory", "leave_out"]);
      const [hub] = withEffectiveFallbacks(referencing, named, { rows: [[{ choice: "directory", directory: "~/a" }]], leftOut: [] }, true);
      expect(hub.account_fallbacks.map((fallback) => fallback.name)).toContain("A");
    });

    it("counts an account that exists already as the one it names", () => {
      const withExisting = [{ ...referencing[0], account_fallbacks: [reference("B")] }];
      expect(withEffectiveFallbacks(withExisting, accounts, initialAccountReview(accounts), true)[0].account_fallbacks).toEqual([]);
    });

    it("stands as the daemon reported it while the accounts are not imported", () => {
      expect(withEffectiveFallbacks(referencing, accounts, initialAccountReview(accounts), false)).toBe(referencing);
      expect(fallenBack(initialAccountReview(accounts), false)).toHaveLength(references.length);
    });
  });
});

describe("the parts of a document", () => {
  it("lists the ones it carries in the dialog's order, and its consoles when it carries nothing at all", () => {
    expect(carriedParts({ consoles, accounts: {}, settings_sections: ["cleanup", "general", "git"] })).toEqual(["consoles", "settings", "accounts"]);
    expect(carriedParts({ consoles: [], accounts: {}, settings_sections: [] })).toEqual(["accounts"]);
    expect(carriedParts({ consoles: [], settings_sections: ["keyboard_shortcuts"] })).toEqual(["settings"]);
    expect(carriedParts({ consoles: [], settings_sections: [] })).toEqual(["consoles"]);
  });

  const unchecked = { consoles: false, accounts: false, settings: false };
  const allSections = { general: true, app_behavior: true, git: true, agent_behavior: true, keyboard_shortcuts: true, cleanup: true };

  it("imports the checked ones, or the only one a document carries whatever its box says", () => {
    const checked = { ...unchecked, consoles: true, settings: true };
    expect(importedParts(["consoles", "settings", "accounts"], checked)).toEqual(["consoles", "settings"]);
    expect(importedParts(["accounts"], checked)).toEqual(["accounts"]);
    expect(importedParts(["consoles", "accounts"], { ...checked, consoles: false })).toEqual([]);
  });

  it("comes to a settings tab while the settings are imported", () => {
    expect(regionsOf(["consoles", "settings", "accounts"])).toEqual(["consoles", "settings", "accounts"]);
    expect(regionsOf(["accounts"])).toEqual(["accounts"]);
    expect(regionsOf([])).toEqual([]);
  });

  it("applies the checked sections in Settings order only while the settings are imported", () => {
    const carries = ["cleanup", "general", "git"] as const;
    expect(importedSections(["consoles", "settings"], carries, { ...allSections, git: false })).toEqual(["general", "cleanup"]);
    expect(importedSections(["consoles"], carries, allSections)).toEqual([]);
    expect(importedSections(["settings"], ["general"], allSections)).toEqual(["general"]);
  });

  it("works out the parts and sections the choices import, and whether the document's own clone directory is taken", () => {
    const document = { consoles, accounts: null, settings_sections: ["general" as const, "git" as const] };
    const checked = { consoles: true, settings: true, accounts: false };
    expect(chosen(document, checked, { ...allSections, general: false })).toEqual({ parts: ["consoles", "settings"], sections: ["git"] });
    expect(takesDocumentCloneDir(document, checked, allSections, true)).toBe(true);
    expect(takesDocumentCloneDir(document, checked, { ...allSections, git: false }, true)).toBe(false);
    expect(takesDocumentCloneDir(document, { ...checked, settings: false }, allSections, true)).toBe(false);
    expect(takesDocumentCloneDir(document, checked, allSections, false)).toBe(false);
  });

  it("has nothing to import when no part is chosen, or only settings with every section unchecked", () => {
    expect(nothingToImport([], [])).toBe(true);
    expect(nothingToImport(["settings"], [])).toBe(true);
    expect(nothingToImport(["settings"], ["git"])).toBe(false);
    expect(nothingToImport(["settings", "accounts"], [])).toBe(false);
    expect(nothingToImport(["consoles"], [])).toBe(false);
  });
});
