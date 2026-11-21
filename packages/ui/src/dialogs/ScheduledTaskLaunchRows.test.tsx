// @vitest-environment jsdom
import { expect, it } from "vitest";

import { sessionOf, snapshotState } from "../fixtures/builders";
import { LanguageProvider } from "../i18n/react";
import type { ScheduledTaskLaunch } from "../protocol";
import type { State } from "../store";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { hasLaunchRows, ScheduledTaskLaunchRows } from "./ScheduledTaskLaunchRows";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const none: ScheduledTaskLaunch = { compatibility: {} };

it.each<{ name: string; launch: ScheduledTaskLaunch; projectTask: boolean; listed: boolean }>([
  { name: "a launch naming no setting", launch: none, projectTask: true, listed: false },
  { name: "a launch that names only a title", launch: { title: "Report", compatibility: {} }, projectTask: true, listed: false },
  { name: "an agent", launch: { agent: "codex", compatibility: {} }, projectTask: true, listed: true },
  { name: "an account, the default one too", launch: { account: null, compatibility: {} }, projectTask: true, listed: true },
  { name: "a model", launch: { model: "m", compatibility: {} }, projectTask: true, listed: true },
  { name: "a reasoning effort", launch: { effort: "high", compatibility: {} }, projectTask: true, listed: true },
  { name: "a session to report to, for a project task", launch: { bound_to: "s-1", compatibility: {} }, projectTask: true, listed: true },
  { name: "a session to report to, for a console task, which has none", launch: { bound_to: "s-1", compatibility: {} }, projectTask: false, listed: false },
  { name: "a compatibility switch", launch: { compatibility: { x: false } }, projectTask: false, listed: true },
])("says whether there is anything to list for $name", ({ launch, projectTask, listed }) => {
  expect(hasLaunchRows(launch, projectTask)).toBe(listed);
});

function mount(launch: ScheduledTaskLaunch, projectTask = true, state: Partial<State> = {}) {
  const base = snapshotState({});
  const container = mountHost(
    <LanguageProvider>
      <dl><ScheduledTaskLaunchRows launch={launch} projectTask={projectTask} /></dl>
    </LanguageProvider>,
    { daemon: testDaemon({ state: { ...base, ...state } }) },
  );
  // Without the marks that isolate each name.
  const text = (element: Element) => (element.textContent ?? "").replace(/[⁦-⁩]/g, "");
  const rows = [...container.querySelectorAll("dt")].map((term) => [text(term), text(term.nextElementSibling!)] as const);
  return { container, rows };
}

it("lists each setting the launch names as plain text, in the order of the session dialogs, and nothing it leaves open", () => {
  const held = sessionOf("s-held", "c-1", "p-1", "Old work", "idle");
  const { container, rows } = mount(
    { agent: "grok", account: null, model: null, effort: null, bound_to: "s-held", compatibility: {} },
    true,
    { sessions: new Map([[held.id, held]]), settings: { ...snapshotState({}).settings!, default_account_names: { grok: "Mine" } } },
  );
  expect(rows).toEqual([
    ["Agent", "Grok Build"],
    ["Account", "Mine"],
    ["Model", "Default"],
    ["Reasoning effort", "Default"],
    ["Report to", "Old work"],
  ]);
  expect(container.querySelectorAll("[data-inline-reference]")).toHaveLength(0);
});

it("names an explicit model and effort as the session dialogs do", () => {
  const { rows } = mount({ agent: "claude", model: "claude-opus-5-5", effort: "high", compatibility: {} });
  expect(rows).toEqual([["Agent", "Claude Code"], ["Model", "Opus 5.5"], ["Reasoning effort", "high"]]);
});

it("calls the default account only the default where the launch names no agent", () => {
  expect(mount({ account: null, compatibility: {} }).rows).toEqual([["Account", "Default"]]);
});

it("lists nothing for a launch that names nothing", () => {
  expect(mount(none).container.querySelector("dt")).toBeNull();
});

it("leaves out the session a console task reports to", () => {
  expect(mount({ agent: "codex", bound_to: "s-1", compatibility: {} }, false).rows).toEqual([["Agent", "Codex"]]);
});

it("shows an account, a session and a switch the window cannot name as plain monospace left-to-right text, never a chip", () => {
  const { container, rows } = mount({ agent: "codex", account: "a-gone", bound_to: "s-gone", compatibility: { from_a_newer_daemon: true } });
  expect(rows).toEqual([["Agent", "Codex"], ["Account", "a-gone"], ["Report to", "s-gone"], ["from_a_newer_daemon", "On"]]);
  const ids = [...container.querySelectorAll<HTMLElement>(".font-mono")];
  expect(ids.map((id) => [id.textContent, id.getAttribute("dir")])).toEqual([["a-gone", "ltr"], ["s-gone", "ltr"], ["from_a_newer_daemon", "ltr"]]);
  expect(container.querySelectorAll("[data-inline-reference]")).toHaveLength(0);
});

it("lists the compatibility switches in the order the provider declares them, then those it does not declare", () => {
  const { rows } = mount({ agent: "grok", compatibility: { zzz_unknown: true, codex_compatibility: true, aaa_unknown: false, claude_compatibility: false } });
  expect(rows.map(([label]) => label.replace(/ for .*/, ""))).toEqual(["Agent", "Claude compatibility", "Codex compatibility", "zzz_unknown", "aaa_unknown"]);
  expect(rows.slice(1).map(([, value]) => value)).toEqual(["Off", "On", "On", "Off"]);
});

it("shows every switch as its id where the launch names no agent", () => {
  const { rows } = mount({ compatibility: { claude_compatibility: true } });
  expect(rows).toEqual([["claude_compatibility", "On"]]);
});
