import { describe, expect, it } from "vitest";

import type { ImportedAccount, ImportedProject } from "../protocol";
import { importDisagreement, withShortcutsEcho } from "./fixtureConfiguration";

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
  settings?: { settings_applied?: { auto_pull_repositories: boolean } },
  status: "created" | "failed" = "created",
) => ({
  ...settings,
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
});

describe("importDisagreement over the consoles", () => {
  it("finds consoles reported that the request asked not to create", () => {
    const request = { type: "import_configuration" as const, document: {}, apply_consoles: false };
    expect(importDisagreement(request, result([project("A", "created", "clone")]))).toMatch(/asked for none/);
  });

  it("finds none where the request asked for no consoles and the result has none, or left the choice out", () => {
    expect(importDisagreement({ type: "import_configuration" as const, document: {}, apply_consoles: false, resolutions: [{ console: 0, project: 0, choice: { kind: "clone" } }] }, { consoles: [] })).toBeUndefined();
    expect(importDisagreement({ type: "import_configuration" as const, document: {} }, { consoles: [] })).toBeUndefined();
  });
});

describe("importDisagreement over accounts", () => {
  const account = (name: string, status: ImportedAccount["status"]): ImportedAccount => ({ name, status, account: status === "created" || status === "existing" ? "a-1" : null, error: null, sharing_warning: null });
  const accounts = (...list: ImportedAccount[]) => ({ agents_added: [], agents: [{ agent: "claude" as const, accounts: list, default_name_applied: false }] });
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

describe("withShortcutsEcho", () => {
  const lock = { code: "KeyK", primary: true, shift: true };
  const asked = (settings: unknown) => ({ type: "import_configuration" as const, document: { settings }, apply_settings: true });
  const applied = result([], { settings_applied: { auto_pull_repositories: true } });

  it("hands back the document's shortcuts in what was applied, an empty object included", () => {
    expect(withShortcutsEcho(asked({ keyboard_shortcuts: { lock } }), applied).settings_applied).toEqual({ auto_pull_repositories: true, keyboard_shortcuts: { lock } });
    expect(withShortcutsEcho(asked({ keyboard_shortcuts: {} }), applied).settings_applied?.keyboard_shortcuts).toEqual({});
  });

  it("echoes nothing for a document without shortcuts or a result that applied no settings", () => {
    expect(withShortcutsEcho(asked({}), applied)).toBe(applied);
    expect(withShortcutsEcho(asked({ keyboard_shortcuts: null }), applied)).toBe(applied);
    const none = result([]);
    expect(withShortcutsEcho(asked({ keyboard_shortcuts: { lock } }), none)).toBe(none);
  });
});
