// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

import { documentSettings } from "../configuration/testSupport";
import type { DocumentSettings, ImportedAccount, ImportedProject, ImportItem, InspectedAgentSettings } from "../protocol";
import { inspected, inspectedProject, REVIEW } from "./configurationReview";
import type { ImportProgressFixture } from "./fixture";
import { handledImportItems, importDisagreement, importProgressDisagreement, withClientValuesEcho } from "./fixtureConfiguration";

const project = (name: string, status: ImportedProject["status"], via: ImportedProject["via"] = null): ImportedProject => ({
  name,
  status,
  via,
  project: status === "created" ? "p-1" : null,
  error: null,
  account_fallbacks: [],
});
const result = (
  projects: ImportedProject[],
  settings?: { settings_applied?: Partial<DocumentSettings> },
  status: "created" | "failed" = "created",
) => ({
  id: null,
  settings_applied: settings?.settings_applied ? documentSettings(settings.settings_applied) : null,
  settings_error: null,
  accounts: null,
  consoles: [{ name: "Work", status, console: status === "created" ? "c-1" : null, error: null, account_fallbacks: [], projects }],
});
const request = (resolutions: { project: number; kind: "exported_path" | "clone" }[], apply_settings?: boolean) => ({
  type: "import_configuration" as const,
  document: {},
  resolutions: resolutions.map(({ project, kind }) => ({ console: 0, project, choice: { kind } })),
  apply_settings,
});

describe("importDisagreement", () => {
  it("finds none where the result follows from the request", () => {
    const projects = [project("A", "created", "exported_path"), project("B", "left_out"), project("C", "failed")];
    expect(importDisagreement(request([{ project: 0, kind: "exported_path" }, { project: 2, kind: "clone" }]), result(projects))).toBeUndefined();
  });

  it("looks into no project of a console that could not be created, whose projects are all not tried whatever was asked", () => {
    const projects = [project("A", "not_attempted"), project("B", "not_attempted")];
    expect(importDisagreement(request([{ project: 0, kind: "clone" }]), result(projects, undefined, "failed"))).toBeUndefined();
  });

  it("finds a project reported left out that the request resolves", () => {
    expect(importDisagreement(request([{ project: 0, kind: "clone" }]), result([project("A", "left_out")]))).toMatch(/A is reported left out/);
  });

  it("finds a project reported created, failed or not tried that the request resolves nothing for", () => {
    for (const status of ["created", "failed", "not_attempted"] as const) {
      expect(importDisagreement(request([]), result([project("A", status, status === "created" ? "clone" : null)])), status).toMatch(/A is reported/);
    }
  });

  it("finds a project reported placed another way than the request resolves it", () => {
    expect(importDisagreement(request([{ project: 0, kind: "clone" }]), result([project("A", "created", "exported_path")]))).toMatch(/placed as exported_path/);
  });

  it("finds application settings reported that the request did not ask for", () => {
    expect(importDisagreement(request([], false), result([], { settings_applied: { auto_pull_repositories: true } }))).toMatch(/did not ask/);
  });

  it("finds application settings asked for that the result does not report", () => {
    expect(importDisagreement(request([], true), result([]))).toMatch(/asked for the application settings/);
  });

  it("finds a clone directory reported written where the settings were not asked for or the machine's own is kept", () => {
    const written = { ...result([], { settings_applied: { auto_pull_repositories: true } }), clone_dir_applied: "/Users/dev/Code" };
    expect(importDisagreement({ ...request([], true), apply_clone_dir: true }, written)).toBeUndefined();
    expect(importDisagreement(request([], true), written)).toBeUndefined();
    expect(importDisagreement({ ...request([], true), apply_clone_dir: false }, written)).toMatch(/clone directory written/);
    const kept = { ...result([], { settings_applied: { default_clone_dir: "/Users/dev/Code" } }) };
    expect(importDisagreement({ ...request([], true), apply_clone_dir: false }, kept)).toMatch(/clone directory among the settings applied/);
    expect(importDisagreement(request([], false), { ...result([]), clone_dir_applied: "/Users/dev/Code" })).toMatch(/clone directory written/);
    const unwritten = result([], { settings_applied: { default_clone_dir: "/Users/dev/Code" } });
    expect(importDisagreement(request([], true), unwritten)).toMatch(/not as the one written/);
    expect(importDisagreement(request([], true), { ...unwritten, clone_dir_applied: "/Users/dev/Other" })).toMatch(/not as the one written/);
    expect(importDisagreement(request([], true), { ...unwritten, clone_dir_applied: "/Users/dev/Code" })).toBeUndefined();
  });
});

describe("importDisagreement over the consoles", () => {
  it("finds consoles reported that the request asked not to create", () => {
    const request = { type: "import_configuration" as const, document: {}, apply_consoles: false };
    expect(importDisagreement(request, result([project("A", "created", "clone")]))).toMatch(/asked for none/);
  });

  it("finds none where the request asked for no consoles and the result has none, or left the choice out", () => {
    expect(importDisagreement({ type: "import_configuration" as const, document: {}, apply_consoles: false, resolutions: [{ console: 0, project: 0, choice: { kind: "clone" } }] }, { ...result([]), consoles: [] })).toBeUndefined();
    expect(importDisagreement({ type: "import_configuration" as const, document: {} }, { ...result([]), consoles: [] })).toBeUndefined();
  });
});

describe("importDisagreement over accounts", () => {
  const account = (name: string, status: ImportedAccount["status"]): ImportedAccount => ({ name, status, account: status === "created" || status === "existing" ? "a-1" : null, error: null });
  const accounts = (...list: ImportedAccount[]) => ({ agents_added: [], agents: [{ agent: "claude" as const, accounts: list, default_name_applied: false, default_disabled_applied: null }], agent_settings_applied: null, agent_settings_error: null });
  const asked = (resolutions: { account: number; leave_out?: boolean; directory?: string }[], apply_accounts?: boolean) => ({
    type: "import_configuration" as const,
    document: {},
    apply_accounts,
    account_resolutions: resolutions.map((resolution) => ({ agent: "claude" as const, ...resolution })),
  });

  it("finds none where the accounts left out are the ones the request leaves out", () => {
    const list = [account("A", "created"), account("B", "left_out"), account("C", "existing"), account("D", "failed")];
    expect(importDisagreement(asked([{ account: 1, leave_out: true }, { account: 3, directory: "~/d" }], true), { ...result([]), accounts: accounts(...list) })).toBeUndefined();
  });

  it("finds an account reported left out that the request does not leave out, and the other way about", () => {
    expect(importDisagreement(asked([], true), { ...result([]), accounts: accounts(account("A", "left_out")) })).toMatch(/A is reported left out/);
    expect(importDisagreement(asked([{ account: 0, leave_out: true }], true), { ...result([]), accounts: accounts(account("A", "created")) })).toMatch(/A is reported created, but the request leaves it out/);
  });

  it("finds accounts reported that the request did not ask for, or asked for and not reported", () => {
    expect(importDisagreement(asked([], false), { ...result([]), accounts: accounts() })).toMatch(/did not ask/);
    expect(importDisagreement(asked([], true), result([]))).toMatch(/asked for the accounts/);
  });
});

describe("withClientValuesEcho", () => {
  const focusMode = { code: "KeyK", primary: true, shift: true };
  const asked = (settings: unknown) => ({ type: "import_configuration" as const, document: { settings }, apply_settings: true });
  const applied = result([], { settings_applied: { auto_pull_repositories: true } });

  it("hands back the document's shortcuts in what was applied, an empty object included", () => {
    expect(withClientValuesEcho(asked({ keyboard_shortcuts: { focusMode } }), applied).settings_applied).toEqual({ ...documentSettings({ auto_pull_repositories: true }), keyboard_shortcuts: { focusMode } });
    expect(withClientValuesEcho(asked({ keyboard_shortcuts: {} }), applied).settings_applied?.keyboard_shortcuts).toEqual({});
  });

  it("hands back the appearance, Markdown style and background running the same way", () => {
    const echoed = withClientValuesEcho(asked({ appearance: "dark", markdown_style: "github", run_in_background: false, interface_language: "ja" }), applied).settings_applied;
    expect(echoed).toEqual({ ...documentSettings({ auto_pull_repositories: true }), appearance: "dark", markdown_style: "github", run_in_background: false });
  });

  it("echoes nothing for a document without them or a result that applied no settings", () => {
    expect(withClientValuesEcho(asked({}), applied)).toBe(applied);
    expect(withClientValuesEcho(asked({ keyboard_shortcuts: null }), applied)).toBe(applied);
    const none = result([]);
    expect(withClientValuesEcho(asked({ keyboard_shortcuts: { focusMode } }), none)).toBe(none);
  });
});

describe("handledImportItems", () => {
  const agentSettings: InspectedAgentSettings = {
    auto_select_account: true,
    current_auto_select_account: false,
    auto_switch_on_usage_limit: false,
    current_auto_switch_on_usage_limit: false,
    auto_start_quota_windows: false,
    current_auto_start_quota_windows: false,
    provider_settings: {},
    current_provider_settings: {},
    agent_tags: {},
    current_agent_tags: {},
  };
  const inspection = inspected({
    settings: REVIEW.settings,
    consoles: [
      { name: "A", account_fallbacks: [], projects: [inspectedProject({ name: "A1", path: "/a1" }), inspectedProject({ name: "A2", path: "/a2" })] },
      { name: "B", account_fallbacks: [], projects: [] },
    ],
    accounts: {
      agents_to_add: [],
      agent_settings: agentSettings,
      agents: [{ agent: "claude", default_name: null, default_name_applies: false, default_disabled: false, default_disabled_applies: false, accounts: ["W", "X", "Y"].map((name) => ({ name, directory: `~/.${name}`, proposed_directory: `/h/.${name}`, status: name === "Y" ? ("existing" as const) : ("new" as const), in_use_by: null, error: null, share_default_config: false, disabled: false })) }],
    },
  });
  const request = { type: "import_configuration" as const, document: {}, import_id: "i" };
  const accountItems: ImportItem[] = ["W", "X", "Y"].map((_, account) => ({ kind: "account", agent: "claude", account }));

  it("lists the clone directory, the accounts not left out (one that exists whatever was chosen for it), the Agent accounts settings, each console with the projects that have a resolution, then the settings, as the daemon announces them", () => {
    expect(
      handledImportItems(
        { ...request, apply_accounts: true, account_resolutions: [{ agent: "claude", account: 0, leave_out: true }, { agent: "claude", account: 2, leave_out: true }], resolutions: [{ console: 0, project: 1, choice: { kind: "clone" } }], apply_settings: true },
        inspection,
      ),
    ).toEqual<ImportItem[]>([
      { kind: "clone_dir" },
      { kind: "account", agent: "claude", account: 1 },
      { kind: "account", agent: "claude", account: 2 },
      { kind: "agent_settings" },
      { kind: "console", console: 0 },
      { kind: "project", console: 0, project: 1 },
      { kind: "console", console: 1 },
      { kind: "settings" },
    ]);
  });

  it("lists no clone directory where this machine's own is kept or the document has no usable one, and no Agent accounts settings the document does not carry", () => {
    const base = { ...request, apply_accounts: true, apply_consoles: false, apply_settings: true };
    expect(handledImportItems({ ...base, apply_clone_dir: false }, inspection)).toEqual([...accountItems, { kind: "agent_settings" }, { kind: "settings" }]);
    const without = { ...inspection, settings: { ...REVIEW.settings!, default_clone_dir: null }, accounts: { ...inspection.accounts!, agent_settings: null } };
    expect(handledImportItems(base, without)).toEqual([...accountItems, { kind: "settings" }]);
  });

  it("lists no console or project where the request asks for none, and no account or settings it does not ask for", () => {
    expect(handledImportItems({ ...request, apply_consoles: false, resolutions: [{ console: 0, project: 0, choice: { kind: "clone" } }] }, inspection)).toEqual([]);
  });
});

describe("importProgressDisagreement", () => {
  const settings: ImportItem = { kind: "settings" };
  const project = (p: number): ImportItem => ({ kind: "project", console: 0, project: p });
  const handled: ImportItem[] = [{ kind: "account", agent: "claude", account: 0 }, { kind: "console", console: 0 }, project(0), project(1), { kind: "console", console: 1 }, settings];
  const report = (item: ImportItem, state: "creating" | "cloning" | "done" | "not_attempted"): ImportProgressFixture => ({ item, state });
  const failed = (item: ImportItem): ImportProgressFixture => ({ item, state: "failed", error: { code: "internal_error", params: {}, message: "x" } });

  it("finds none for an import part-way: what is behind it settled, the console of the running project done as soon as it was created, the rest waiting", () => {
    expect(
      importProgressDisagreement(handled, [report(handled[0], "done"), report(handled[1], "done"), failed(project(0)), report(project(1), "cloning")]),
    ).toBeUndefined();
    expect(importProgressDisagreement(handled, [])).toBeUndefined();
  });

  it("finds none for a console being created, whose projects still wait", () => {
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), report(handled[1], "creating")])).toBeUndefined();
  });

  it("finds none for a console that could not be created, whose projects were not tried", () => {
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), failed(handled[1]), report(project(0), "not_attempted"), report(project(1), "not_attempted"), report(handled[4], "creating")])).toBeUndefined();
  });

  it("finds an item that is not handled, or reported twice", () => {
    expect(importProgressDisagreement(handled, [report({ kind: "console", console: 5 }, "done")])).toMatch(/does not handle/);
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), report(handled[0], "creating")])).toMatch(/twice/);
  });

  it("finds an item that has moved on while one before it still waits", () => {
    expect(importProgressDisagreement(handled, [report(settings, "creating")])).toMatch(/after an item that still waits/);
    expect(importProgressDisagreement(handled, [report(project(0), "done")])).toMatch(/after an item that still waits/);
  });

  it("finds a project that moved on while its console is not done, or one not tried under a console that was created", () => {
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), report(handled[1], "creating"), report(project(0), "creating")])).toMatch(/its console is creating/);
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), report(handled[1], "done"), report(project(0), "not_attempted")])).toMatch(/its console is done/);
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), failed(handled[1]), report(project(0), "done")])).toMatch(/its console is failed/);
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), failed(handled[1]), report(project(0), "not_attempted")])).toMatch(/still waits though its console failed/);
  });

  it("finds a state only a project has on another item", () => {
    expect(importProgressDisagreement(handled, [report(handled[0], "cloning")])).toMatch(/cannot be cloning/);
    expect(importProgressDisagreement(handled, [report(handled[0], "done"), report(handled[1], "not_attempted")])).toMatch(/cannot be not_attempted/);
  });
});
