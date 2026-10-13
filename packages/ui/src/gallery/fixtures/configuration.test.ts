import { expect, it } from "vitest";

import type { Imported } from "../../configuration/ImportResult";
import type { Inspection } from "../../configuration/ImportReview";
import type { AccountFallback } from "../../protocol";
import { SCENARIOS } from ".";
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
    expect(inspection.settings!.current_auto_sync_repositories, id).toBe(machine!.auto_sync_repositories);
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
      expect(result.settings_applied.auto_sync_repositories, id).toBe(inspection.settings.auto_sync_repositories);
    }
  }
});
