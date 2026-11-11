// @vitest-environment jsdom
import { useState } from "react";

import { expect, it } from "vitest";

import type { PlainMessageKey } from "../i18n/catalog";
import { mountHost } from "../test/mountHost";
import { SettingRow } from "./SettingRow";
import { SettingsGroup } from "./SettingsGroup";
import { LeadRows } from "./SettingsLead";
import type { SectionMatches, SettingsSearchId } from "./settingsSearch";
import { SettingsSearchBlock, SettingsSearchProvider } from "./settingsSearchFilter";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const matches = (ids: SettingsSearchId[], groups: PlainMessageKey[]): SectionMatches => ({ ids: new Set(ids), groups: new Set(groups) });

const rowFor = (container: HTMLElement, id: string) => container.querySelector<HTMLElement>(`[data-settings-search-id="${id}"]`)!;
const groupFor = (container: HTMLElement, title: string) => container.querySelector<HTMLElement>(`[data-settings-search-group="${title}"]`)!;
/** Whether a group is on screen as a group: its heading shown, and the group named by it. The group element itself is
 * never hidden, so that its rows can be seen leaving. */
const groupShown = (group: HTMLElement) => !(group.firstElementChild as HTMLElement).hidden && group.hasAttribute("aria-labelledby");

function Section() {
  return (
    <>
      <SettingsGroup title="settings.git.group.repository">
        <SettingRow searchId="clone-dir" label="Alpha" />
        <SettingRow searchId="auto_pull_repositories" label="Beta" />
      </SettingsGroup>
      <SettingsGroup title="settings.git.group.protection">
        <SettingRow searchId="git_workflow.revert_gate" label="Gamma" />
      </SettingsGroup>
      <div data-unlisted><SettingRow label="Unlisted" /></div>
      <SettingsSearchBlock id="trusted-folders.list">Block content</SettingsSearchBlock>
    </>
  );
}

it("shows every row, group and block outside a search", () => {
  const container = mountHost(<Section />);
  expect([...container.querySelectorAll("[hidden]")]).toEqual([]);
});

it("shows every row, group and block where no matches are in force", () => {
  const container = mountHost(<SettingsSearchProvider matches={undefined}><Section /></SettingsSearchProvider>);
  expect([...container.querySelectorAll("[hidden]")]).toEqual([]);
});

it("hides a row, group and block the matches leave out, and shows those they hold", () => {
  const container = mountHost(
    <SettingsSearchProvider matches={matches(["clone-dir"], ["settings.git.group.repository"])}><Section /></SettingsSearchProvider>,
  );
  expect(rowFor(container, "clone-dir").hidden).toBe(false);
  expect(rowFor(container, "auto_pull_repositories").hidden).toBe(true);
  expect(rowFor(container, "git_workflow.revert_gate").hidden).toBe(true);
  expect(groupShown(groupFor(container, "settings.git.group.repository"))).toBe(true);
  expect(groupShown(groupFor(container, "settings.git.group.protection"))).toBe(false);
  expect(rowFor(container, "trusted-folders.list").hidden).toBe(true);
});

it("shows a block the matches hold", () => {
  const container = mountHost(<SettingsSearchProvider matches={matches(["trusted-folders.list"], [])}><Section /></SettingsSearchProvider>);
  expect(rowFor(container, "trusted-folders.list").hidden).toBe(false);
  expect(rowFor(container, "trusted-folders.list").textContent).toBe("Block content");
});

it("hides the heading of a group none of whose rows is found, with its rows, and no longer names the group by it", () => {
  const container = mountHost(<SettingsSearchProvider matches={matches([], [])}><Section /></SettingsSearchProvider>);
  const group = groupFor(container, "settings.git.group.protection");
  const heading = group.firstElementChild as HTMLElement;
  expect(heading.hidden).toBe(true);
  expect(heading.textContent).toContain("Protection");
  expect(group.hasAttribute("aria-labelledby")).toBe(false);
  expect(rowFor(container, "git_workflow.revert_gate").hidden).toBe(true);
  // Kept shown, holding nothing that is, so that what leaves it can be seen leaving.
  expect(group.hidden).toBe(false);
});

it("never hides a row that has no entry of its own", () => {
  const container = mountHost(<SettingsSearchProvider matches={matches([], [])}><Section /></SettingsSearchProvider>);
  const row = container.querySelector<HTMLElement>("[data-unlisted]")!.firstElementChild as HTMLElement;
  expect(row.textContent).toBe("Unlisted");
  expect(row.hidden).toBe(false);
});

it("keeps a hidden row mounted, so what was typed into it survives being filtered out and back", () => {
  function Field() {
    const [value, setValue] = useState("");
    return <input aria-label="Draft" value={value} onChange={(event) => setValue(event.target.value)} />;
  }
  const row = (
    <SettingRow searchId="clone-dir" label="Alpha"><Field /></SettingRow>
  );
  const container = mountHost(<SettingsSearchProvider matches={matches(["clone-dir"], [])}>{row}</SettingsSearchProvider>);
  const input = container.querySelector("input")!;
  // Typed through the value setter, as the platform does.
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "half-typed");
  input.dispatchEvent(new Event("input", { bubbles: true }));
  container.render(<SettingsSearchProvider matches={matches([], [])}>{row}</SettingsSearchProvider>);
  expect(rowFor(container, "clone-dir").hidden).toBe(true);
  expect(container.querySelector("input")).toBe(input);
  container.render(<SettingsSearchProvider matches={matches(["clone-dir"], [])}>{row}</SettingsSearchProvider>);
  expect(rowFor(container, "clone-dir").hidden).toBe(false);
  expect(container.querySelector("input")).toBe(input);
  expect(input.value).toBe("half-typed");
});

it("keeps a hidden block's content mounted", () => {
  const container = mountHost(<SettingsSearchProvider matches={matches([], [])}><SettingsSearchBlock id="trusted-folders.list"><input aria-label="Draft" /></SettingsSearchBlock></SettingsSearchProvider>);
  const input = container.querySelector("input")!;
  container.render(<SettingsSearchProvider matches={matches(["trusted-folders.list"], [])}><SettingsSearchBlock id="trusted-folders.list"><input aria-label="Draft" /></SettingsSearchBlock></SettingsSearchProvider>);
  expect(container.querySelector("input")).toBe(input);
});

it("hides the lead's rows wrapper, rules and all, when asked to", () => {
  const container = mountHost(
    <>
      <LeadRows hidden><SettingRow label="Alpha" /></LeadRows>
      <LeadRows ruledBelow><SettingRow label="Beta" /></LeadRows>
    </>,
  );
  const [hidden, shown] = [...container.children] as HTMLElement[];
  expect(hidden!.hidden).toBe(true);
  expect(shown!.hidden).toBe(false);
});
