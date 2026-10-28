import { expect, it } from "vitest";

import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/ImportReview";
import type { AccountFallback } from "../../protocol";
import { SCENARIOS } from ".";
import { effectiveClaudeCompatibility } from "./builders";
import { DEFAULT_CLONE_DIR } from "./configuration";

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
    expect(inspection.settings!.current_auto_pull_repositories, id).toBe(machine!.auto_pull_repositories);
    expect(inspection.settings!.current_auto_delete_unresumable_archives, id).toBe(machine!.auto_delete_unresumable_archives);
    if (typeof inspection.settings!.auto_select_account === "boolean") {
      expect(inspection.settings!.current_auto_select_account, id).toBe(machine!.auto_select_account ?? false);
    }
    if (typeof inspection.settings!.auto_switch_on_usage_limit === "boolean") {
      expect(inspection.settings!.current_auto_switch_on_usage_limit, id).toBe(machine!.auto_switch_on_usage_limit ?? false);
    }
    if (typeof inspection.settings!.sidebar_order_by_status === "boolean") {
      expect(inspection.settings!.current_sidebar_order_by_status, id).toBe(machine!.sidebar_order_by_status ?? true);
    }
    if (typeof inspection.settings!.auto_start_quota_windows === "boolean") {
      expect(inspection.settings!.current_auto_start_quota_windows, id).toBe(machine!.auto_start_quota_windows ?? false);
    }
    if (typeof inspection.settings!.auto_approve_session_requests === "boolean") {
      expect(inspection.settings!.current_auto_approve_session_requests, id).toBe(machine!.auto_approve_session_requests ?? false);
    }
    if (typeof inspection.settings!.auto_approve_resume_requests === "boolean") {
      expect(inspection.settings!.current_auto_approve_resume_requests, id).toBe(machine!.auto_approve_resume_requests ?? false);
    }
    if (typeof inspection.settings!.auto_approve_account_switches === "boolean") {
      expect(inspection.settings!.current_auto_approve_account_switches, id).toBe(machine!.auto_approve_account_switches ?? false);
    }
    if (typeof inspection.settings!.auto_approve_handoffs === "boolean") {
      expect(inspection.settings!.current_auto_approve_handoffs, id).toBe(machine!.auto_approve_handoffs ?? false);
    }
    if (inspection.settings!.git_workflow) {
      expect(inspection.settings!.current_git_workflow, id).toEqual(machine!.git_workflow);
    }
    if (inspection.settings!.claude_compatibility) {
      // The values in effect on this machine, not its explicit choices alone.
      expect(inspection.settings!.current_claude_compatibility, id).toEqual(effectiveClaudeCompatibility(machine!.claude_compatibility ?? {}, machine!.added_agents));
    }
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

it("reports a result for the consoles and projects of the inspection it follows, in its order", () => {
  expect(imported.length).toBeGreaterThan(0);
  for (const { id, inspection, result } of imported) {
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

it("reports the account fallbacks the inspection announced, for what was created", () => {
  for (const { inspection, result } of imported) {
    result.consoles.forEach((console, c) => {
      if (console.status === "created") expect(console.account_fallbacks, console.name).toEqual(inspection.consoles[c].account_fallbacks);
      console.projects.forEach((project, p) => {
        if (project.status === "created") expect(project.account_fallbacks, project.name).toEqual(inspection.consoles[c].projects[p].account_fallbacks);
      });
    });
  }
});

it("reports application settings only for a document that carries them", () => {
  for (const { id, inspection, result } of imported) {
    if (inspection.settings === undefined || inspection.settings === null) {
      expect(result.settings_applied ?? null, id).toBeNull();
      expect(result.settings_error ?? null, id).toBeNull();
    } else if (result.settings_applied) {
      expect(result.settings_applied.auto_pull_repositories, id).toBe(inspection.settings.auto_pull_repositories);
      expect(result.settings_applied.auto_delete_unresumable_archives, id).toBe(inspection.settings.auto_delete_unresumable_archives);
      if (typeof inspection.settings.auto_select_account === "boolean") {
        expect(result.settings_applied.auto_select_account, id).toBe(inspection.settings.auto_select_account);
      }
      if (typeof inspection.settings.auto_switch_on_usage_limit === "boolean") {
        expect(result.settings_applied.auto_switch_on_usage_limit, id).toBe(inspection.settings.auto_switch_on_usage_limit);
      }
      if (typeof inspection.settings.sidebar_order_by_status === "boolean") {
        expect(result.settings_applied.sidebar_order_by_status, id).toBe(inspection.settings.sidebar_order_by_status);
      }
      if (typeof inspection.settings.auto_start_quota_windows === "boolean") {
        expect(result.settings_applied.auto_start_quota_windows, id).toBe(inspection.settings.auto_start_quota_windows);
      }
      if (typeof inspection.settings.auto_approve_session_requests === "boolean") {
        expect(result.settings_applied.auto_approve_session_requests, id).toBe(inspection.settings.auto_approve_session_requests);
      }
      if (typeof inspection.settings.auto_approve_resume_requests === "boolean") {
        expect(result.settings_applied.auto_approve_resume_requests, id).toBe(inspection.settings.auto_approve_resume_requests);
      }
      if (typeof inspection.settings.auto_approve_account_switches === "boolean") {
        expect(result.settings_applied.auto_approve_account_switches, id).toBe(inspection.settings.auto_approve_account_switches);
      }
      if (typeof inspection.settings.auto_approve_handoffs === "boolean") {
        expect(result.settings_applied.auto_approve_handoffs, id).toBe(inspection.settings.auto_approve_handoffs);
      }
      if (inspection.settings.git_workflow) {
        expect(result.settings_applied.git_workflow, id).toEqual(inspection.settings.git_workflow);
      }
      if (inspection.settings.claude_compatibility) {
        expect(result.settings_applied.claude_compatibility, id).toEqual(inspection.settings.claude_compatibility);
      }
    }
  }
});
