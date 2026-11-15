// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { withModality } from "../components/dialog/dialogTestSupport";
import { accountOf, sessionOf } from "../fixtures/builders";
import type { Session } from "../protocol";
import { fakeTimers } from "../test/clock";
import { buttons, click } from "../test/dom";
import { mountHost } from "../test/mountHost";
import { testDaemon } from "../test/testDaemon";
import { SwitchAccountDialog } from "./SwitchAccountDialog";

vi.mock("../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "light" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const target = accountOf("claude-work", "claude", "Work", "/accounts/work");
const parent = sessionOf("parent", "console", undefined, "Coordinator", "idle");
const child = sessionOf("child", "console", "project", "Child", "idle", { bound_to: parent.id });
const unrelated = sessionOf("unrelated", "console", "other-project", "Unrelated", "idle");
const sessionsOf = (sessions: Session[]) => new Map(sessions.map((session) => [session.id, session]));
const radios = () => Array.from(document.querySelectorAll<HTMLInputElement>('input[type="radio"]'));
const confirm = () => buttons().find((button) => button.textContent === "Switch")!;

function mount(sessions: Session[], selected: Session = parent) {
  withModality("pointer");
  fakeTimers({ now: new Date("2026-11-16T00:00:00Z") });
  const daemon = testDaemon({ state: { sessions: sessionsOf(sessions) } });
  daemon.store.setState({ settings: { ...daemon.store.getState().settings, accounts: [target] } });
  const onSwitchAccounts = vi.fn();
  const onClose = vi.fn();
  mountHost(<SwitchAccountDialog session={selected} account={target.id} accountName={target.name} onClose={onClose} onSwitchAccounts={onSwitchAccounts} />, { daemon });
  return { store: daemon.store, onSwitchAccounts, onClose };
}

it("omits redundant scope controls for one session and switches it directly", async () => {
  const { onSwitchAccounts, onClose } = mount([parent]);
  expect(radios()).toHaveLength(0);
  expect(document.querySelector('[role="radiogroup"]')).toBeNull();
  expect(document.body.textContent).not.toContain("This session and its child sessions");
  expect(confirm().disabled).toBe(false);
  await click(confirm());
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([parent.id], target.id, target.name);
  expect(onClose).toHaveBeenCalledOnce();
});

it("shows the descendant scope and count as information when both scopes contain the same eligible sessions", async () => {
  const archived = { ...child, id: "archived", status: "archived" as const };
  const grandchild = { ...child, id: "grandchild", bound_to: archived.id };
  const ineligible = [
    { ...unrelated, id: "already-target", account_id: target.id },
    { ...unrelated, id: "archived-unrelated", status: "archived" as const },
    { ...unrelated, id: "other-agent", agent: "codex" },
    { ...unrelated, id: "other-console", console_id: "elsewhere" },
    { ...unrelated, id: "switching", switching_account: true },
  ];
  const { store, onSwitchAccounts } = mount([parent, child, archived, grandchild, ...ineligible, unrelated]);
  act(() => store.setState({ queuedAccountSwitches: new Set([unrelated.id]) }));
  expect(radios()).toHaveLength(0);
  expect(document.body.textContent).toContain("This session and its child sessions");
  expect(document.body.textContent).toContain("3 sessions can switch.");
  expect(document.body.textContent).toContain("Each session's process restarts under the new account.");
  await click(confirm());
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([parent.id, child.id, grandchild.id], target.id, target.name);
});

it("offers both scopes when an eligible unrelated session exists, defaulting to the session's descendants", async () => {
  const { onSwitchAccounts } = mount([parent, child, unrelated]);
  expect(radios().map((radio) => radio.checked)).toEqual([true, false]);
  await click(confirm());
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([parent.id, child.id], target.id, target.name);
});

it("updates scope choices live and reads the chosen console targets when confirmed", async () => {
  const { store, onSwitchAccounts } = mount([parent, child]);
  expect(radios()).toHaveLength(0);
  act(() => store.setState({ sessions: sessionsOf([parent, child, unrelated]) }));
  expect(radios().map((radio) => radio.checked)).toEqual([true, false]);
  await click(radios().find((radio) => radio.value === "console")!);
  const openedLater = { ...unrelated, id: "opened-later" };
  act(() => store.setState({ sessions: sessionsOf([parent, child, unrelated, openedLater]) }));
  await click(confirm());
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([parent.id, child.id, unrelated.id, openedLater.id], target.id, target.name);
});

it("falls back to the session scope when a previously selected console scope becomes redundant", async () => {
  const { store, onSwitchAccounts } = mount([parent, unrelated]);
  await click(radios().find((radio) => radio.value === "console")!);
  act(() => store.setState({ sessions: sessionsOf([parent, { ...unrelated, account_id: target.id }]) }));
  expect(radios()).toHaveLength(0);
  expect(document.body.textContent).toContain("Its process restarts under the new account and the conversation continues there.");
  await click(confirm());
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([parent.id], target.id, target.name);
});

it("does not reuse a hidden console selection to include unrelated sessions during the launch confirmation", async () => {
  const { store, onSwitchAccounts } = mount([parent, child, unrelated]);
  await click(radios().find((radio) => radio.value === "console")!);
  act(() => store.setState({
    sessions: sessionsOf([parent, child]),
    settings: { ...store.getState().settings, accounts: [{ ...target, share_default_config_incomplete: true }] },
  }));
  expect(radios()).toHaveLength(0);
  await click(confirm());
  const startAnyway = buttons().find((button) => button.textContent === "Start anyway")!;
  expect(startAnyway).toBeDefined();
  expect(onSwitchAccounts).not.toHaveBeenCalled();
  act(() => store.setState({ sessions: sessionsOf([parent, child, unrelated]) }));
  await click(startAnyway);
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([parent.id, child.id], target.id, target.name);
});

it("disables confirmation when the only scope has no eligible sessions left", () => {
  const { store, onSwitchAccounts } = mount([parent, child]);
  act(() => store.setState({ queuedAccountSwitches: new Set([parent.id, child.id]) }));
  expect(radios()).toHaveLength(0);
  expect(document.body.textContent).toContain("This session and its child sessions");
  expect(document.body.textContent).toContain("0 sessions can switch.");
  expect(confirm().disabled).toBe(true);
  expect(onSwitchAccounts).not.toHaveBeenCalled();
});

it("keeps the fresh-conversation explanation when only one unused session can switch", () => {
  mount([{ ...parent, has_conversation: false }], { ...parent, has_conversation: false });
  expect(radios()).toHaveLength(0);
  expect(document.body.textContent).toContain("with a fresh conversation, as nothing has been typed into it yet.");
});

it("keeps the console choice available after the selected session is deleted", async () => {
  const { store, onSwitchAccounts } = mount([parent, unrelated]);
  act(() => store.setState({ sessions: sessionsOf([unrelated]) }));
  expect(radios().map((radio) => radio.checked)).toEqual([true, false]);
  expect(document.body.textContent).toContain("0 sessions can switch.");
  expect(confirm().disabled).toBe(true);
  await click(confirm());
  expect(onSwitchAccounts).not.toHaveBeenCalled();
  await click(radios().find((radio) => radio.value === "console")!);
  expect(confirm().disabled).toBe(false);
  await click(confirm());
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([unrelated.id], target.id, target.name);
});

it("disables confirmation when deleting the selected session leaves no targets", async () => {
  const { store, onSwitchAccounts } = mount([parent]);
  act(() => store.setState({ sessions: new Map() }));
  expect(radios()).toHaveLength(0);
  expect(document.body.textContent).toContain("This session and its child sessions");
  expect(document.body.textContent).toContain("0 sessions can switch.");
  expect(confirm().disabled).toBe(true);
  await click(confirm());
  expect(onSwitchAccounts).not.toHaveBeenCalled();
});
