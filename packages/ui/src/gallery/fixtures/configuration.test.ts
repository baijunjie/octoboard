import { expect, it } from "vitest";

import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/importPlan";
import { DEFAULT_ACCOUNT_NAME, type AccountFallback } from "../../protocol";
import { SCENARIOS } from ".";
import { effectiveClaudeCompatibility } from "./builders";
import { carriedSettings, DEFAULT_CLONE_DIR, resolvedAccountDirectory } from "./configuration";

type Shown<T> = { id: string; machine: NonNullable<NonNullable<(typeof SCENARIOS)[number]["state"]>["settings"]> | undefined } & T;

/** Every inspection the scenarios answer with, and the machine (its settings) of the scenario that
 * answers it. */
const inspected: Shown<{ inspection: Omit<Inspection, "type"> }>[] = SCENARIOS.flatMap((s) => {
  const inspection = s.configuration?.inspection;
  return inspection && !("code" in inspection) ? [{ id: s.id, machine: s.state?.settings, inspection }] : [];
});
/** Every import result the scenarios answer with, beside the inspection of the same scenario. */
const imported: Shown<{ inspection: Omit<Inspection, "type">; result: Omit<Imported, "type"> }>[] = SCENARIOS.flatMap((s) => {
  const { inspection, imported: result } = s.configuration ?? {};
  return inspection && !("code" in inspection) && typeof result === "object" && !("code" in result)
    ? [{ id: s.id, machine: s.state?.settings, inspection, result }]
    : [];
});
const projectsOf = (inspection: Omit<Inspection, "type">) => inspection.consoles.flatMap((console) => console.projects);

/** What a clone of `url` is named, as the daemon names it: the last segment of its path without
 * `.git`. */
const repoName = (url: string) => url.replace(/\/+$/, "").split(/[/:]/).at(-1)!.replace(/\.git$/, "");
const lastSegment = (path: string) => path.split("/").filter(Boolean).at(-1);
/** Where each agent's default account keeps its configuration on the machines of these scenarios. */
const DEFAULT_ACCOUNT_DIRECTORY = { claude: "/Users/dev/.claude", codex: "/Users/dev/.codex", grok: "/Users/dev/.grok" };
const sameName = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

// What the fixture states by hand has to be what the daemon states: a gallery that shows a project
// preselected on something it does not offer would settle copy and layout on a state that cannot
// occur.
it("preselects the first of the exported path, the default clone directory and the clone that a project offers, and nothing where it offers none", () => {
  const projects = inspected.flatMap(({ inspection }) => projectsOf(inspection));
  expect(projects.length).toBeGreaterThan(0);
  for (const { offers, default: preselected, name } of projects) {
    const first = (["exported_path", "default_clone_dir", "clone"] as const).find((choice) => offers[choice] !== null) ?? null;
    expect(preselected, name).toBe(first);
  }
});

it("offers a clone only for a project that records a remote", () => {
  for (const { offers, remote_url, name } of inspected.flatMap(({ inspection }) => projectsOf(inspection))) {
    if (offers.clone !== null) expect(remote_url, name).not.toBeNull();
  }
});

it("offers the recorded path as the directory it is, the derived directory under the default clone directory by the recorded path's last segment, and a clone under it by the repository's name", () => {
  for (const { offers, path, remote_url, name } of inspected.flatMap(({ inspection }) => projectsOf(inspection))) {
    if (offers.exported_path !== null) expect(offers.exported_path, name).toBe(path);
    if (offers.default_clone_dir !== null) expect(offers.default_clone_dir, name).toBe(`${DEFAULT_CLONE_DIR}/${lastSegment(path)}`);
    if (offers.clone !== null) expect(offers.clone, name).toBe(`${DEFAULT_CLONE_DIR}/${repoName(remote_url!)}`);
  }
});

it("never offers a clone into a directory it also offers as one that exists", () => {
  for (const { offers, name } of inspected.flatMap(({ inspection }) => projectsOf(inspection))) {
    if (offers.clone !== null) expect(offers.default_clone_dir, name).not.toBe(offers.clone);
  }
});

it("reports the setting this machine has now as the scenario's own state has it", () => {
  const withSettings = inspected.filter(({ inspection }) => inspection.settings);
  expect(withSettings.length).toBeGreaterThan(0);
  for (const { id, machine, inspection } of withSettings) {
    const settings = inspection.settings!;
    expect(settings.current_auto_pull_repositories, id).toBe(machine!.auto_pull_repositories);
    expect(settings.current_auto_delete_unresumable_archives, id).toBe(machine!.auto_delete_unresumable_archives);
    expect(settings.current_auto_select_account, id).toBe(machine!.auto_select_account ?? false);
    expect(settings.current_auto_switch_on_usage_limit, id).toBe(machine!.auto_switch_on_usage_limit ?? false);
    expect(settings.current_sidebar_order_by_status, id).toBe(machine!.sidebar_order_by_status ?? true);
    expect(settings.current_auto_start_quota_windows, id).toBe(machine!.auto_start_quota_windows ?? false);
    expect(settings.current_auto_approve_session_requests, id).toBe(machine!.auto_approve_session_requests ?? false);
    expect(settings.current_auto_approve_resume_requests, id).toBe(machine!.auto_approve_resume_requests ?? false);
    expect(settings.current_auto_approve_account_switches, id).toBe(machine!.auto_approve_account_switches ?? false);
    expect(settings.current_auto_approve_handoffs, id).toBe(machine!.auto_approve_handoffs ?? false);
    expect(settings.current_git_workflow, id).toEqual(machine!.git_workflow);
    // The values in effect on this machine, not its explicit choices alone.
    expect(settings.current_claude_compatibility, id).toEqual(effectiveClaudeCompatibility(machine!.claude_compatibility ?? {}, machine!.added_agents));
  }
});

it("lets an account fall back only where this machine has no account of that agent by that name, whatever its case", () => {
  for (const { id, machine, inspection } of inspected) {
    const fallbacks: AccountFallback[] = inspection.consoles.flatMap((console) => [
      ...console.account_fallbacks,
      ...console.projects.flatMap((project) => project.account_fallbacks),
    ]);
    for (const { agent, name } of fallbacks) {
      expect(machine!.accounts.some((account) => account.agent === agent && sameName(account.name, name)), `${id}: ${agent} ${name}`).toBe(false);
    }
  }
});

it("reports a result as the daemon does: what placed a created project, why a failed one failed, and fallbacks only for what was created", () => {
  const projects = imported.flatMap(({ result }) => result.consoles.flatMap((console) => console.projects.map((project) => ({ console, project }))));
  expect(projects.length).toBeGreaterThan(0);
  for (const { console, project } of projects) {
    expect(project.status === "created", project.name).toBe(project.via !== null && project.project !== null);
    expect(project.status === "failed", project.name).toBe(project.error !== null);
    if (project.status !== "created") expect(project.account_fallbacks, project.name).toEqual([]);
    if (console.status === "failed") expect(project.status, project.name).toBe("not_attempted");
  }
});

it("reports a console that could not be created with its reason, no id and no fallbacks, and a created one with its id", () => {
  for (const { result } of imported) {
    for (const console of result.consoles) {
      expect(console.status === "failed", console.name).toBe(console.error !== null);
      expect(console.console === null, console.name).toBe(console.status === "failed");
      if (console.status === "failed") expect(console.account_fallbacks, console.name).toEqual([]);
    }
  }
});

it("reports a result for the consoles and projects of the inspection it follows, in its order, or for none where the import was asked to create none", () => {
  expect(imported.length).toBeGreaterThan(0);
  for (const { id, inspection, result } of imported) {
    // An import without consoles (`apply_consoles` false, which the fixture daemon holds the request to) creates none.
    if (result.consoles.length === 0) continue;
    const shape = (consoles: { name: string; projects: { name: string }[] }[]) =>
      consoles.map((console) => [console.name, console.projects.map((project) => project.name)]);
    expect(shape(result.consoles), id).toEqual(shape(inspection.consoles));
  }
});

it("reports a created project as placed the way it was offered, or in a directory the user gave", () => {
  for (const { inspection, result } of imported) {
    result.consoles.forEach((console, c) =>
      console.projects.forEach((project, p) => {
        if (project.via === null || project.via === "directory") return;
        expect(inspection.consoles[c].projects[p].offers[project.via], project.name).not.toBeNull();
      }),
    );
  }
});

it("reports the account fallbacks the inspection announced, for what was created, less the references the accounts the import created now resolve", () => {
  for (const { inspection, result } of imported) {
    // The inspection lists a reference to an account the document would create too, as it matches nothing yet. Once that
    // account is created the reference binds to it: it still falls back where the account was left out or failed, or
    // where the accounts were not imported at all.
    const stands = (fallback: AccountFallback) =>
      !result.accounts?.agents.some((agent) => agent.agent === fallback.agent && agent.accounts.some((account) => account.status === "created" && sameName(account.name, fallback.name)));
    const reported = (fallbacks: AccountFallback[]) => fallbacks.filter(stands);
    result.consoles.forEach((console, c) => {
      if (console.status === "created") expect(console.account_fallbacks, console.name).toEqual(reported(inspection.consoles[c].account_fallbacks));
      console.projects.forEach((project, p) => {
        if (project.status === "created") expect(project.account_fallbacks, project.name).toEqual(reported(inspection.consoles[c].projects[p].account_fallbacks));
      });
    });
  }
});

const accountsOf = (inspection: Omit<Inspection, "type">) => inspection.accounts?.agents ?? [];

const accounted = inspected.flatMap(({ id, machine, inspection }) => (inspection.accounts ? [{ id, machine: machine!, accounts: inspection.accounts }] : []));

it("resolves the directory of an account as the daemon does, and gives a status only the facts of this machine allow", () => {
  expect(accounted.length).toBeGreaterThan(0);
  for (const { id, machine, accounts } of accounted) {
    for (const { agent, accounts: list } of accounts.agents) {
      const here = machine.accounts.filter((account) => account.agent === agent);
      for (const account of list) {
        const label = `${id}: ${agent} ${account.name}`;
        expect(account.proposed_directory, label).toBe(resolvedAccountDirectory(account.directory));
        expect(account.status === "invalid", label).toBe(account.error !== null);
        expect(account.status === "directory_in_use", label).toBe(account.in_use_by !== null);
        const named = here.some((existing) => sameName(existing.name, account.name));
        expect(account.status === "existing", label).toBe(named);
        if (account.status === "new") {
          expect(here.some((existing) => existing.config_dir === account.proposed_directory), label).toBe(false);
        }
        if (account.status === "directory_in_use") {
          // The default account is not among the accounts of its own: it is in use where its agent keeps its configuration.
          const directory = account.in_use_by === DEFAULT_ACCOUNT_NAME ? DEFAULT_ACCOUNT_DIRECTORY[agent] : here.find((existing) => existing.name === account.in_use_by)?.config_dir;
          expect(directory, label).toBe(account.proposed_directory);
        }
      }
    }
  }
});

it("adds only agents this machine has not added, and applies a default name only where the agent's default account has none and nothing collides", () => {
  for (const { id, machine, accounts } of accounted) {
    for (const agent of accounts.agents_to_add) expect(machine.added_agents ?? [], `${id}: ${agent}`).not.toContain(agent);
    for (const entry of accounts.agents) {
      if (!entry.default_name_applies) continue;
      expect(entry.default_name, `${id}: ${entry.agent}`).not.toBeNull();
      expect(machine.default_account_names?.[entry.agent], `${id}: ${entry.agent}`).toBeUndefined();
      const names = [...machine.accounts.filter((account) => account.agent === entry.agent), ...entry.accounts].map((account) => account.name.toLowerCase());
      expect(names, `${id}: ${entry.agent}`).not.toContain(entry.default_name!.toLowerCase());
    }
  }
});

it("reports a result of the accounts for the accounts of the inspection it follows, in its order, as the daemon reports them", () => {
  const withAccounts = imported.filter(({ result }) => result.accounts);
  expect(withAccounts.length).toBeGreaterThan(0);
  for (const { id, inspection, result } of withAccounts) {
    const shape = (agents: { agent: string; accounts: { name: string }[] }[]) => agents.map((entry) => [entry.agent, entry.accounts.map((account) => account.name)]);
    expect(shape(result.accounts!.agents), id).toEqual(shape(accountsOf(inspection)));
    result.accounts!.agents.forEach((entry, a) =>
      entry.accounts.forEach((account, i) => {
        const label = `${id}: ${entry.agent} ${account.name}`;
        const proposed = inspection.accounts!.agents[a].accounts[i];
        expect(account.status === "existing", label).toBe(proposed.status === "existing");
        if (account.status === "created") expect(proposed.status, label).not.toBe("existing");
        expect(account.status === "failed", label).toBe(account.error !== null);
        expect(account.account !== null, label).toBe(account.status === "created" || account.status === "existing");
        if (account.sharing_warning) expect(account.status, label).toBe("created");
      }),
    );
    for (const agent of result.accounts!.agents_added) expect(inspection.accounts!.agents_to_add, id).toContain(agent);
    result.accounts!.agents.forEach((entry, a) => {
      if (entry.default_name_applied) expect(inspection.accounts!.agents[a].default_name_applies, `${id}: ${entry.agent}`).toBe(true);
    });
  }
});

it("reports accounts only for a document that carries them", () => {
  for (const { id, inspection, result } of imported) {
    if (!inspection.accounts) expect(result.accounts ?? null, id).toBeNull();
  }
});

it("reports application settings only for a document that carries them", () => {
  for (const { id, inspection, result } of imported) {
    if (inspection.settings === undefined || inspection.settings === null) {
      expect(result.settings_applied ?? null, id).toBeNull();
      expect(result.settings_error ?? null, id).toBeNull();
    } else if (result.settings_applied) {
      const { keyboard_shortcuts: _shortcuts, ...applied } = result.settings_applied;
      expect(applied, id).toEqual(carriedSettings(inspection.settings));
    }
  }
});
