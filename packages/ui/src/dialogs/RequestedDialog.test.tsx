// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { expect, it, vi } from "vitest";

import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "../components/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { Account, Session, SessionBranchItem } from "../protocol";
import { DaemonRequestError } from "../daemon-client";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import type { DialogRequest } from "./dialogRequest";
import { RequestedDialog } from "./RequestedDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const owner = sessionOf("s-owner", console_.id, project.id, "Owner", "idle");
const account: Account = { id: "a-1", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" };

/** Mounts the dialog for `dialog` over a store holding `sessions` and `accounts`. */
function mount(
  dialog: DialogRequest,
  sessions: Session[],
  accounts: Account[] = [],
  callbacks: { onClose?: () => void; request?: Daemon["request"]; onSwitchAccounts?: (sessionIds: string[], account: string | null, accountName: string) => void; strict?: boolean } = {},
): { container: HTMLElement; daemon: Daemon } {
  const daemon: Daemon = {
    store: createStateStore({
      sessions: new Map(sessions.map((s) => [s.id, s])),
      settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "", accounts },
    }),
    request: vi.fn(callbacks.request) as never,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const content = <DaemonProvider value={daemon}>
    <RequestedDialog dialog={dialog} onClose={callbacks.onClose ?? (() => {})} onSessionOpened={() => {}} onResumeSession={async () => {}} onSwitchAccounts={callbacks.onSwitchAccounts ?? (() => {})} />
  </DaemonProvider>;
  const container = mountForTooltip(callbacks.strict ? <StrictMode>{content}</StrictMode> : content);
  return { container, daemon };
}

/** The agent icon's tooltip trigger in the callout's first listed session. */
function agentIcon(): HTMLElement {
  return document.body.querySelector<SVGElement>("[role=alertdialog] li svg[fill-rule=evenodd]")!.parentElement!;
}

function hoverAgentIcon(): string | undefined {
  const settle = withFakeTimers();
  enter(agentIcon());
  settle();
  const text = openTooltip()?.textContent ?? undefined;
  leave(agentIcon());
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
  return text;
}

// Each of the three confirmations that list sessions, over the session it lists.
const CONFIRMATIONS: Array<[string, DialogRequest, Session]> = [
  ["removing a project", { kind: "delete-project", project }, sessionOf("s-1", console_.id, project.id, "Running", "working")],
  [
    "archiving a session",
    { kind: "archive-session", session: owner },
    sessionOf("s-2", console_.id, project.id, "Stopped", "interrupted", { bound_to: owner.id }),
  ],
  [
    "deleting an archived session",
    { kind: "delete-session", session: { ...owner, status: "archived" } },
    sessionOf("s-3", console_.id, project.id, "Gone", "archived", { bound_to: owner.id }),
  ],
];

it.each(CONFIRMATIONS)("says which account a listed session runs under when %s", (_, dialog, listed) => {
  mount(dialog, [owner, listed]);
  expect(hoverAgentIcon()).toBe("Claude Code, Default");
});

it.each([
  ["a named account", { account_id: account.id }, "Claude Code, Work"],
  ["an account that is no longer stored", { account_id: "gone", config_dir: "/home/dev/.claude-old" }, "Claude Code, /home/dev/.claude-old"],
])("says the account of a session on %s", (_, extra, text) => {
  const listed = sessionOf("s-2", console_.id, project.id, "Stopped", "interrupted", { bound_to: owner.id, ...extra });
  mount({ kind: "archive-session", session: owner }, [owner, listed], [account]);
  expect(hoverAgentIcon()).toBe(text);
});

it("leaves the screen-reader-only name of the agent beside the icon as it was", () => {
  const listed = sessionOf("s-2", console_.id, project.id, "Stopped", "interrupted", { bound_to: owner.id, account_id: account.id });
  mount({ kind: "archive-session", session: owner }, [owner, listed], [account]);
  const row = document.body.querySelector("[role=alertdialog] li")!;
  expect(row.querySelector(".sr-only")?.textContent).toBe(`Claude Code (${isolate("Work")})`);
  expect(agentIcon().hasAttribute("aria-label")).toBe(false);
});

// The glyph and the icon sit a few pixels apart in one row: moving from one to the other inside
// the tooltip window's cooldown opens the second at once, and after the cooldown it waits again.
it.each([
  { pause: 100, opensAtOnce: true },
  { pause: 1000, opensAtOnce: false },
])("hands the glyph's tooltip over to the agent icon's after a pause of $pause ms: at once $opensAtOnce", ({ pause, opensAtOnce }) => {
  const listed = sessionOf("s-2", console_.id, project.id, "Stopped", "interrupted", { bound_to: owner.id });
  mount({ kind: "archive-session", session: owner }, [owner, listed]);
  const settle = withFakeTimers();
  const glyph = document.body.querySelector("[role=alertdialog] li")!.children[0];

  enter(glyph);
  settle();
  expect(openTooltip()?.textContent).toBe("Interrupted");
  // As a real pointer does, it leaves the glyph before it reaches the icon.
  leave(glyph);
  act(() => void vi.advanceTimersByTime(pause));
  enter(agentIcon());

  expect(openTooltip()?.textContent ?? null).toBe(opensAtOnce ? "Claude Code, Default" : null);
  settle();
  expect(openTooltip()?.textContent).toBe("Claude Code, Default");

  leave(agentIcon());
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});


it("edits the created session name while the agent has a dynamic terminal title", () => {
  const session = { ...owner, terminal_title: "Explain the gallery" };
  mount({ kind: "rename-session", session }, [session]);
  const input = document.body.querySelector<HTMLInputElement>('[role="dialog"] input');
  expect(input?.value).toBe(owner.title);
});


it.each(["archive-session", "delete-session", "switch-account"] as const)("updates the %s confirmation heading when the terminal title changes", (kind) => {
  const session = { ...owner, terminal_title: "First task" };
  const dialog: DialogRequest = kind === "switch-account"
    ? { kind, session, account: account.id, accountName: account.name }
    : { kind, session };
  const { daemon } = mount(dialog, [session]);
  const title = () => document.body.querySelector('[role="alertdialog"] h2')?.textContent;
  expect(title()).toContain("First task");
  act(() => daemon.store.setState({ sessions: new Map([[session.id, { ...session, terminal_title: "Next task" }]]) }));
  expect(title()).toContain(`${isolate("Next task")}｜${isolate(session.title)}`);
  expect(title()).not.toContain("First task");
});

function deferred(): { promise: Promise<void>; resolve: () => void } {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

/** The bodies of the requests of `type` the dialog sent. */
function sent(daemon: Daemon, type: string): unknown[] {
  return vi.mocked(daemon.request).mock.calls.map(([body]) => body).filter((body) => body.type === type);
}

async function submitDialog(): Promise<void> {
  // An inspection the confirmation holds Confirm for settles first.
  await act(async () => {});
  await act(async () => {
    document.body.querySelector("[role=alertdialog] form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  });
}

it("lists every bound descendant before archiving and refreshes the list while open", async () => {
  const hub = sessionOf("hub", console_.id, undefined, "Console", "idle", { role: "console" });
  const manual = sessionOf("manual", console_.id, project.id, "Manual session", "working", { bound_to: hub.id, origin: "user" });
  const lead = sessionOf("lead", console_.id, project.id, "Lead session", "idle", { bound_to: hub.id });
  const nested = sessionOf("nested", console_.id, project.id, "Nested session", "waiting_user", { bound_to: lead.id });
  const archived = sessionOf("archived", console_.id, project.id, "Archived session", "archived", { bound_to: hub.id });
  const elsewhere = sessionOf("elsewhere", console_.id, project.id, "Unbound session", "working");
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "archive-session", session: hub }, [hub, manual, lead, nested, archived, elsewhere], [], {
    onClose, request: async () => ({ type: "ack" }),
  });
  const titles = () => [...document.querySelectorAll("[role=alertdialog] li")].map((row) => row.textContent);
  expect(titles()).toHaveLength(3);
  for (const title of [manual.title, lead.title, nested.title]) expect(titles().some((text) => text?.includes(title))).toBe(true);
  expect(sent(daemon, "archive_session")).toEqual([]);
  const late = sessionOf("late", console_.id, project.id, "New session", "starting", { bound_to: hub.id });
  act(() => daemon.store.setState({ sessions: new Map([...daemon.store.getState().sessions, [late.id, late]]) }));
  expect(titles()).toHaveLength(4);
  expect(document.querySelector("[role=alertdialog]")?.textContent).toContain("Their processes end.");
  await submitDialog();
  expect(sent(daemon, "archive_session")).toEqual([{ type: "archive_session", session: hub.id }]);
  expect(onClose).toHaveBeenCalledOnce();
});

function checkbox(): HTMLInputElement {
  return document.body.querySelector<HTMLInputElement>('[role=alertdialog] input[type="checkbox"]')!;
}

function checkAll(): void {
  act(() => checkbox().click());
}

it("moves the project folder to the Trash only when the checkbox is selected", async () => {
  const { daemon } = mount({ kind: "delete-project", project }, []);
  expect(checkbox().checked).toBe(false);
  await submitDialog();
  expect(daemon.request).toHaveBeenLastCalledWith({ type: "delete_project", project: project.id, stop_sessions: false, trash_directory: false });
  act(() => checkbox().click());
  await submitDialog();
  expect(daemon.request).toHaveBeenLastCalledWith({ type: "delete_project", project: project.id, stop_sessions: false, trash_directory: true });
});

it("moves the agent's records to the Trash only when the checkbox is selected", async () => {
  const archived = { ...owner, status: "archived" as const };
  const { daemon } = mount({ kind: "delete-session", session: archived }, [archived]);
  expect(checkbox().checked).toBe(false);
  await submitDialog();
  expect(daemon.request).toHaveBeenLastCalledWith({ type: "delete_session", session: archived.id, trash_agent_records: false });
  act(() => checkbox().click());
  await submitDialog();
  expect(daemon.request).toHaveBeenLastCalledWith({ type: "delete_session", session: archived.id, trash_agent_records: true });
});

it("moves the agents' records to the Trash only when the checkbox of deleting archived sessions is selected", async () => {
  const { daemon } = mount({ kind: "delete-archived", console: console_, project, count: 2 }, []);
  expect(checkbox().checked).toBe(false);
  await submitDialog();
  expect(daemon.request).toHaveBeenLastCalledWith({ type: "delete_archived_sessions", console: console_.id, project: project.id, trash_agent_records: false });
  act(() => checkbox().click());
  await submitDialog();
  expect(daemon.request).toHaveBeenLastCalledWith({ type: "delete_archived_sessions", console: console_.id, project: project.id, trash_agent_records: true });
});

/** The confirm button. */
const confirmButton = () => document.body.querySelector("[data-slot=alert-dialog-footer] button:last-child");
/** The confirm button's text. */
const confirmText = () => confirmButton()?.textContent;

const archivedOwner = { ...owner, status: "archived" as const };
// Each confirmation with a Trash option: its sessions, its confirm label before and after the option
// is ticked, and the request it sends.
const TRASH_CONFIRMATIONS: Array<[string, DialogRequest, Session[], string, string, string]> = [
  ["deleting a session", { kind: "delete-session", session: archivedOwner }, [archivedOwner], "Delete", "Delete all", "delete_session"],
  ["deleting all archived sessions", { kind: "delete-archived", console: console_, project, count: 2 }, [], "Delete all", "Delete all", "delete_archived_sessions"],
  ["removing a project", { kind: "delete-project", project }, [], "Remove", "Remove", "delete_project"],
  ["removing a project that has running sessions", { kind: "delete-project", project }, [{ ...owner, status: "working" }], "End and remove", "End and remove", "delete_project"],
];

it.each(TRASH_CONFIRMATIONS)("gives confirming %s its label for a ticked Trash option, and keeps it while it runs", async (_, dialog, sessions, plain, ticked, sentType) => {
  const { daemon } = mount(dialog, sessions, [], {
    // Inspections settle; the deletion itself never does, so the dialog stays busy.
    request: async (body) => {
      if (body.type === "inspect_session_branches") return { type: "session_branches_inspected", items: [] };
      if (body.type === "inspect_session_records") return { type: "session_records_inspected", presence: "present" };
      if (body.type === "inspect_project_removal") return { type: "project_removal_inspected", items: [] };
      return new Promise(() => {});
    },
  });
  await act(async () => {});
  expect(confirmText()).toBe(plain);
  checkAll();
  await act(async () => {});
  expect(checkbox().checked).toBe(true);
  expect(confirmText()).toBe(ticked);
  await submitDialog();
  expect(sent(daemon, sentType)).toHaveLength(1);
  expect(confirmText()).toBe(ticked);
});

it.each<[string, DialogRequest, Session[], string, string]>([
  ["archiving a session", { kind: "archive-session", session: owner }, [owner], "Archive", "Archive and delete"],
  ["deleting a session", { kind: "delete-session", session: archivedOwner }, [archivedOwner], "Delete", "Delete all"],
  ["deleting all archived sessions", { kind: "delete-archived", console: console_, project, count: 1 }, [archivedOwner], "Delete all", "Delete all"],
])("labels confirming %s for a ticked force option, and back once it is unticked", async (_, dialog, sessions, plain, ticked) => {
  mount(dialog, sessions, [], {
    request: async (body) => body.type === "inspect_session_branches"
      ? { type: "session_branches_inspected", items: [UNCOMMITTED, UNMERGED] }
      : { type: "session_records_inspected", presence: "absent" },
  });
  await act(async () => {});
  expect(confirmText()).toBe(plain);
  checkAll();
  expect(checkbox().checked).toBe(true);
  expect(confirmText()).toBe(ticked);
  checkAll();
  expect(confirmText()).toBe(plain);
});

it("draws the confirm button of archiving in the danger colour only while a deletion option is ticked", async () => {
  mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", items: [UNCOMMITTED, UNMERGED] }),
  });
  await act(async () => {});
  const danger = () => confirmButton()!.className.includes("danger");
  expect(danger()).toBe(false);
  checkAll();
  expect(danger()).toBe(true);
  checkAll();
  expect(danger()).toBe(false);
});

it("labels archiving for deletion when only locked worktrees are ticked", async () => {
  mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", items: [LOCKED] }),
  });
  await act(async () => {});
  checkAll();
  expect(confirmText()).toBe("Archive and delete");
});

const hub = sessionOf("s-hub", console_.id, undefined, "Hub", "idle", { role: "console" });
const lead = sessionOf("s-lead", console_.id, project.id, "Lead", "idle", { bound_to: hub.id });
const bound = sessionOf("s-bound", console_.id, project.id, "Bound", "idle", { bound_to: lead.id });
const unrelated = sessionOf("s-unrelated", console_.id, project.id, "Unrelated", "idle");
const excluded = [
  { ...owner, id: "already", account_id: account.id, bound_to: hub.id },
  { ...owner, id: "archived", status: "archived" as const, bound_to: hub.id },
  { ...owner, id: "other-agent", agent: "codex" as const, bound_to: hub.id },
  { ...owner, id: "other-console", console_id: "c-other" },
];

function chooseScope(label: RegExp): void {
  act(() => [...document.body.querySelectorAll<HTMLInputElement>('input[type="radio"]')].find((radio) => label.test(radio.closest("label")?.textContent ?? ""))!.click());
}

it("switches only the selected session, and closes at once, unless another scope is chosen", async () => {
  const selected = { ...owner, id: "s-sel", account_id: account.id };
  const onClose = vi.fn();
  const onSwitchAccounts = vi.fn();
  mount({ kind: "switch-account", session: selected, account: null, accountName: "Default" }, [selected, { ...selected, id: "s-other" }], [account], { onClose, onSwitchAccounts });
  const radios = [...document.body.querySelectorAll<HTMLInputElement>('input[type="radio"]')];
  expect(radios.map((radio) => radio.checked)).toEqual([true, false]);
  expect(document.body.querySelector('[role="progressbar"]')).toBeNull();
  await submitDialog();
  expect(onSwitchAccounts).toHaveBeenCalledExactlyOnceWith([selected.id], null, "Default");
  expect(onClose).toHaveBeenCalledOnce();
});

it("switches the session with every session bound under it, however deep, and not the ones that cannot switch", async () => {
  const onClose = vi.fn();
  const onSwitchAccounts = vi.fn();
  const archivedLead = { ...lead, id: "s-archived-lead", status: "archived" as const, bound_to: hub.id };
  const underArchived = { ...bound, id: "s-under-archived", bound_to: archivedLead.id };
  mount({ kind: "switch-account", session: hub, account: account.id, accountName: account.name }, [hub, lead, bound, archivedLead, underArchived, unrelated, ...excluded], [account], { onClose, onSwitchAccounts });
  expect(document.body.textContent).toContain("4 sessions can switch");
  await submitDialog();
  expect(onSwitchAccounts).toHaveBeenCalledOnce();
  const [ids, target] = onSwitchAccounts.mock.calls[0] as [string[], string, string];
  expect([...ids].sort()).toEqual([bound.id, hub.id, lead.id, underArchived.id].sort());
  expect(target).toBe(account.id);
  expect(onClose).toHaveBeenCalledOnce();
});

it("switches every eligible session of the agent in the console when that scope is chosen", async () => {
  const onSwitchAccounts = vi.fn();
  mount({ kind: "switch-account", session: lead, account: account.id, accountName: account.name }, [hub, lead, bound, unrelated, ...excluded], [account], { onSwitchAccounts });
  expect(document.body.textContent).toContain("2 sessions can switch");
  chooseScope(/Every session in this console/);
  await submitDialog();
  const [ids] = onSwitchAccounts.mock.calls[0] as [string[]];
  expect([...ids].sort()).toEqual([bound.id, hub.id, lead.id, unrelated.id].sort());
});

it.each(["claude", "codex", "grok"] as const)("deletes an unused %s session and its records without inspection or confirmation", async (agent) => {
  const unused = { ...owner, agent, has_conversation: false, agent_session_id: agent === "codex" ? undefined : owner.agent_session_id };
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose,
    strict: true,
    request: async () => ({ type: "ack" }),
  });
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledExactlyOnceWith({ type: "delete_session", session: unused.id, only_if_unused: true });
  expect(onClose).toHaveBeenCalledOnce();
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
});

it("also deletes an archived unused session without confirmation", async () => {
  const unused = { ...owner, status: "archived" as const, has_conversation: false };
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose, request: async () => ({ type: "ack" }),
  });
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledExactlyOnceWith({ type: "delete_session", session: unused.id, only_if_unused: true });
  expect(onClose).toHaveBeenCalledOnce();
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
});

it("keeps the confirmation for a used archived session but hides cleanup when no files exist", async () => {
  const archived = { ...owner, status: "archived" as const };
  mount({ kind: "delete-session", session: archived }, [archived], [], {
    request: async () => ({ type: "session_records_inspected", presence: "absent" }),
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).toBeNull();
});

it("reports a refused unused deletion without offering an unguarded confirmation", async () => {
  const unused = { ...owner, has_conversation: false };
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose,
    request: async () => { throw new Error("Session is now in use"); },
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
  expect(daemon.request).toHaveBeenCalledTimes(1);
  expect(daemon.toastError).toHaveBeenCalledOnce();
  expect(onClose).toHaveBeenCalledOnce();
});

it("checks the native records of archived children as well as their owner", async () => {
  const archived = { ...owner, status: "archived" as const };
  const child = sessionOf("s-child", console_.id, project.id, "Child", "archived", { bound_to: owner.id });
  const { daemon } = mount({ kind: "delete-session", session: archived }, [archived, child], [], {
    request: async () => ({ type: "session_records_inspected", presence: "present" }),
  });
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledWith({ type: "inspect_session_records", sessions: [child.id, owner.id].sort() });
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).not.toBeNull();
});


it("keeps the bulk deletion confirmation but removes cleanup for a file-free scope", async () => {
  const archived = { ...owner, status: "archived" as const };
  const { daemon } = mount({ kind: "delete-archived", console: console_, project, count: 1 }, [archived], [], {
    request: async () => ({ type: "session_records_inspected", presence: "absent" }),
  });
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledWith({ type: "inspect_session_records", sessions: [archived.id] });
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).toBeNull();
});

it("retains confirmation and cleanup when inspecting records fails", async () => {
  const archived = { ...owner, status: "archived" as const };
  const { daemon } = mount({ kind: "delete-session", session: archived }, [archived], [], {
    request: async () => { throw new Error("Connection lost"); },
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).not.toBeNull();
  expect(sent(daemon, "inspect_session_records")).toHaveLength(1);
});


it.each(["single", "bulk"] as const)("keeps keyboard focus in the %s confirmation when cleanup disappears", async (kind) => {
  const archived = { ...owner, status: "archived" as const };
  let resolve!: (value: { type: "session_records_inspected"; presence: "absent" }) => void;
  const waiting = new Promise<{ type: "session_records_inspected"; presence: "absent" }>((done) => { resolve = done; });
  mount(kind === "single"
    ? { kind: "delete-session", session: archived }
    : { kind: "delete-archived", console: console_, project, count: 1 }, [archived], [], {
    request: () => waiting,
  });
  const option = checkbox();
  act(() => option.focus());
  expect(document.activeElement).toBe(option);
  await act(async () => { resolve({ type: "session_records_inspected", presence: "absent" }); });
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).toBeNull();
  expect(document.activeElement).toBe(document.querySelector("[role=alertdialog]"));
});

it("does not close a used session's confirmation when an earlier direct deletion finishes", async () => {
  const unused = { ...owner, has_conversation: false };
  const waiting = deferred();
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose,
    request: async (body) => {
      if (body.type === "delete_session") { await waiting.promise; return { type: "ack" }; }
      return { type: "session_records_inspected", presence: "absent" };
    },
  });
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, owner]]) }));
  await act(async () => waiting.resolve());
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(sent(daemon, "delete_session")).toHaveLength(1);
  expect(sent(daemon, "inspect_session_records")).toHaveLength(1);
  expect(onClose).not.toHaveBeenCalled();
});

const UNCOMMITTED: SessionBranchItem = {
  kind: "worktree", repository: "/r/.git", branch: "feature", path: "/r-feature", project: project.id,
  session: owner.id, state: "uncommitted_changes", uncommitted: true, detail: null,
};
const UNMERGED: SessionBranchItem = { ...UNCOMMITTED, kind: "branch", path: null, state: "unmerged", uncommitted: false };
const KEPT: SessionBranchItem = { ...UNMERGED, branch: "main", state: "checked_out" };
const LOCKED: SessionBranchItem = { ...UNCOMMITTED, branch: "pinned", path: "/r-pinned", state: "locked", uncommitted: false };
const PINNED: SessionBranchItem = { ...UNMERGED, branch: "pinned", state: "removable" };

/** `item` as a request names it back. */
function targetOf({ kind, repository, branch, path, state, uncommitted }: SessionBranchItem) {
  return { kind, repository, branch, path, state, uncommitted };
}

/** A row's text as assistive technology reads it: what is hidden from it left out. */
function announced(row: Element): string {
  const copy = row.cloneNode(true) as Element;
  copy.querySelectorAll("[aria-hidden=true]").forEach((hidden) => hidden.remove());
  // Screen readers skip the bidi isolates around substituted values.
  return (copy.textContent ?? "").replace(/[\u2066-\u2069]/g, "");
}
const listed = () => [...document.querySelectorAll("[role=alertdialog] li")].map(announced);
const boxes = () => [...document.querySelectorAll<HTMLInputElement>('[role=alertdialog] input[type="checkbox"]')];

// Each confirmation that removes the bound branches of its sessions, with the request it sends.
const BRANCH_CONFIRMATIONS: Array<[string, DialogRequest, Record<string, unknown>]> = [
  ["archiving a session", { kind: "archive-session", session: owner }, { type: "archive_session", session: owner.id }],
  ["deleting an archived session", { kind: "delete-session", session: { ...owner, status: "archived" } },
    { type: "delete_session", session: owner.id, trash_agent_records: false }],
  ["deleting all archived sessions", { kind: "delete-archived", console: console_, project, count: 1 },
    { type: "delete_archived_sessions", console: console_.id, project: project.id, trash_agent_records: false }],
];

it.each(BRANCH_CONFIRMATIONS)("lists what git refuses to remove when %s and forces it only once the checkbox is selected", async (_, dialog, body) => {
  const archived = { ...owner, status: "archived" as const };
  const { daemon } = mount(dialog, [dialog.kind === "archive-session" ? owner : archived], [], {
    request: async (sentBody) => sentBody.type === "inspect_session_branches"
      ? { type: "session_branches_inspected", items: [UNCOMMITTED, UNMERGED, KEPT] }
      : { type: "session_records_inspected", presence: "absent" },
  });
  await act(async () => {});
  expect(sent(daemon, "inspect_session_branches")).toEqual([{ type: "inspect_session_branches", sessions: [owner.id] }]);
  expect(listed()).toEqual(["/r-feature on branch feature", "feature"]);
  const text = document.querySelector("[role=alertdialog]")?.textContent;
  expect(text).toContain("This worktree has uncommitted changes:");
  expect(text).toContain("This branch is not merged:");
  expect(checkbox().checked).toBe(false);
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ ...body, keep: [targetOf(UNCOMMITTED), targetOf(UNMERGED)] });
  checkAll();
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ ...body, force_remove: [targetOf(UNCOMMITTED), targetOf(UNMERGED)] });
});

it("offers nothing to force when everything bound can go or has to stay", async () => {
  mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", items: [{ ...UNMERGED, state: "removable" }, KEPT] }),
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).toBeNull();
});

it("lists locked worktrees apart, with their changes, under a checkbox of their own that forces them alone", async () => {
  const { daemon } = mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", items: [UNMERGED, { ...LOCKED, uncommitted: true }] }),
  });
  await act(async () => {});
  expect(boxes()).toHaveLength(2);
  expect(listed()).toContain("/r-pinned on branch pinned, uncommitted changes");
  act(() => boxes()[1].click());
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({
    type: "archive_session", session: owner.id,
    force_remove: [targetOf({ ...LOCKED, uncommitted: true })], keep: [targetOf(UNMERGED)],
  });
});

it("unchecks a choice once the list it was made over changes, and keeps the list up while asking again", async () => {
  const late = sessionOf("s-late", console_.id, project.id, "Late", "idle", { bound_to: owner.id });
  const second = deferred();
  const { daemon } = mount({ kind: "archive-session", session: { ...owner, status: "working" } }, [owner], [], {
    request: async (body) => {
      if (body.type !== "inspect_session_branches") return { type: "ack" };
      if (body.sessions.length === 1) return { type: "session_branches_inspected", items: [UNMERGED] };
      await second.promise;
      return { type: "session_branches_inspected", items: [UNMERGED, { ...UNMERGED, branch: "late" }] };
    },
  });
  await act(async () => {});
  checkAll();
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, owner], [late.id, late]]) }));
  // The newer question is on its way: the list stands, and Confirm waits.
  expect(listed()).toContain("feature");
  expect(checkbox().checked).toBe(true);
  await act(async () => document.body.querySelector("[role=alertdialog] form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(sent(daemon, "archive_session")).toEqual([]);
  await act(async () => second.resolve());
  expect(listed()).toContain("late");
  expect(checkbox().checked).toBe(false);
});

it("drops an answer that arrives after a newer question", async () => {
  const late = sessionOf("s-late", console_.id, project.id, "Late", "idle", { bound_to: owner.id });
  const first = deferred();
  const { daemon } = mount({ kind: "archive-session", session: { ...owner, status: "working" } }, [owner], [], {
    request: async (body) => {
      if (body.type !== "inspect_session_branches") return { type: "ack" };
      if (body.sessions.length === 1) {
        await first.promise;
        return { type: "session_branches_inspected", items: [{ ...UNMERGED, branch: "stale" }] };
      }
      return { type: "session_branches_inspected", items: [UNMERGED] };
    },
  });
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, owner], [late.id, late]]) }));
  await act(async () => {});
  await act(async () => first.resolve());
  expect(listed()).toContain("feature");
  expect(listed()).not.toContain("stale");
});

it("lists what goes along with a project's folder, asked afresh each time moving it to the Trash is chosen", async () => {
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async () => ({ type: "project_removal_inspected", items: [UNCOMMITTED, UNMERGED] }),
  });
  await act(async () => {});
  expect(sent(daemon, "inspect_project_removal")).toEqual([]);
  checkAll();
  await act(async () => {});
  expect(listed()).toEqual(["/r-feature on branch feature", "feature"]);
  const text = document.querySelector("[role=alertdialog]")?.textContent;
  expect(text).toContain("Moved to the Trash with the folder:");
  expect(text).toContain("Deleted permanently:");
  checkAll();
  expect(listed()).toEqual([]);
  checkAll();
  await act(async () => {});
  expect(sent(daemon, "inspect_project_removal")).toHaveLength(2);
});

it("holds a project's removal while what goes along is asked, and says more may go when that fails", async () => {
  const answer = deferred();
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type !== "inspect_project_removal") return { type: "ack" };
      await answer.promise;
      throw new Error("Connection lost");
    },
  });
  checkAll();
  await act(async () => document.body.querySelector("[role=alertdialog] form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(sent(daemon, "delete_project")).toEqual([]);
  await act(async () => answer.resolve());
  expect(document.querySelector("[role=alertdialog]")?.textContent).toContain("its branches deleted permanently");
  await submitDialog();
  expect(sent(daemon, "delete_project")).toHaveLength(1);
});

it("takes a locked worktree, and the branch it has checked out, along with a project only when chosen", async () => {
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async () => ({ type: "project_removal_inspected", items: [UNCOMMITTED, LOCKED, PINNED] }),
  });
  checkAll();
  await act(async () => {});
  expect(listed()).toEqual(["/r-feature on branch feature", "/r-pinned on branch pinned"]);
  act(() => boxes()[1].click());
  expect(listed()).toContain("pinned");
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({
    type: "delete_project", project: project.id, stop_sessions: false, trash_directory: true,
    force_remove: [targetOf(LOCKED)],
  });
  // Without the folder nothing goes along, whatever was ticked for it.
  act(() => boxes()[0].click());
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "delete_project", project: project.id, stop_sessions: false, trash_directory: false });
});

/** The warning boxes the confirmation shows. */
const warnings = () => [...document.querySelectorAll("[role=alertdialog] [data-slot=alert-root]")];
const warningContent = () => warnings()[0].parentElement!;

it("says that more may go along with a project in the dialog's one warning", async () => {
  mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type === "inspect_project_removal") throw new Error("Connection lost");
      return { type: "ack" };
    },
  });
  expect(warnings()).toHaveLength(0);
  checkAll();
  await act(async () => {});
  expect(warnings()).toHaveLength(1);
  expect(warningContent().textContent).toContain("deleted permanently");
});

it("adds what a ticked option does after the dialog's one warning, which holds no checkbox", async () => {
  const child = sessionOf("s-child", console_.id, project.id, "Child", "archived", { bound_to: owner.id });
  mount({ kind: "delete-session", session: { ...owner, status: "archived" } }, [owner, child], [], {
    request: async (body) => body.type === "inspect_session_branches"
      ? { type: "session_branches_inspected", items: [UNCOMMITTED, UNMERGED, LOCKED] }
      : { type: "session_records_inspected", presence: "absent" },
  });
  await act(async () => {});
  expect(warnings()).toHaveLength(1);
  expect(warnings()[0].querySelector("input[type=checkbox], ul")).toBeNull();
  expect(warnings()[0].querySelector('[data-slot="alert-content"]')?.children).toHaveLength(1);
  act(() => boxes()[0].click());
  act(() => boxes()[1].click());
  expect(warnings()).toHaveLength(1);
  expect(warningContent().textContent).toContain("Also permanently deletes 2 worktrees and branches Git keeps.");
  expect(warnings()[0].textContent).not.toContain("Also permanently deletes");
  for (const box of boxes()) {
    for (const id of (box.getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean)) {
      expect(document.getElementById(id)).not.toBeNull();
      expect(document.getElementById(id)?.closest('[data-slot="alert-root"]')).toBeNull();
    }
  }
  expect(warningContent().textContent).toContain("Also permanently deletes the locked worktree, uncommitted changes included.");
  act(() => boxes()[0].click());
  expect(warningContent().textContent).not.toContain("worktrees and branches Git keeps");
});

it("shows a warning only once an option is ticked where the confirmation has none of its own", async () => {
  mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", items: [UNMERGED] }),
  });
  await act(async () => {});
  expect(warnings()).toHaveLength(0);
  checkAll();
  expect(warnings()).toHaveLength(1);
  checkAll();
  expect(warnings()).toHaveLength(0);
});

it("forces no locked worktree once the Trash option is unticked after the locked one was ticked", async () => {
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async () => ({ type: "project_removal_inspected", items: [LOCKED] }),
  });
  checkAll();
  await act(async () => {});
  act(() => boxes()[1].click());
  expect(boxes()[1].checked).toBe(true);
  act(() => boxes()[0].click());
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "delete_project", project: project.id, stop_sessions: false, trash_directory: false });
});

it("warns of nothing going along with a project whose removal would be refused", async () => {
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type === "inspect_project_removal") throw new DaemonRequestError("shared", "project_directory_shared", { project: "p-2" });
      return { type: "ack" };
    },
  });
  checkAll();
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")?.textContent).not.toContain("deleted permanently");
  expect(listed()).toEqual([]);
  await submitDialog();
  expect(sent(daemon, "delete_project")).toHaveLength(1);
});

/** The disclosure button under an option, by the words it shows. */
function disclosure(text: string): HTMLButtonElement {
  return [...document.querySelectorAll<HTMLButtonElement>("[role=alertdialog] button[aria-expanded]")].find((button) => button.textContent?.includes(text))!;
}

it("folds what an option covers under a Details button described by its option, left as the user leaves it", async () => {
  const late = sessionOf("s-late", console_.id, project.id, "Late", "idle", { bound_to: owner.id });
  const { daemon } = mount({ kind: "archive-session", session: { ...owner, status: "working" } }, [owner], [], {
    request: async (body) => {
      if (body.type !== "inspect_session_branches") return { type: "ack" };
      return { type: "session_branches_inspected", items: body.sessions.length === 1 ? [UNMERGED, LOCKED] : [UNMERGED, LOCKED, { ...UNMERGED, branch: "late" }] };
    },
  });
  await act(async () => {});
  const details = () => [...document.querySelectorAll<HTMLButtonElement>("[role=alertdialog] button[aria-expanded]")];
  expect(details().map((button) => button.textContent)).toEqual(["Details", "Details"]);
  expect(details()[0].getAttribute("aria-expanded")).toBe("false");
  expect(details()[0].closest("h1, h2, h3, h4, h5, h6")).toBeNull();
  const description = document.getElementById(details()[0].getAttribute("aria-describedby")!)!;
  expect(description.textContent).toContain("Also force-remove the worktree or branch Git keeps");
  act(() => details()[0].click());
  // Ticking another option, or the list changing, leaves it open.
  act(() => boxes()[1].click());
  expect(details()[0].getAttribute("aria-expanded")).toBe("true");
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, owner], [late.id, late]]) }));
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")?.textContent).toContain("Also force-remove the worktrees and branches Git keeps (2)");
  expect(details()[0].getAttribute("aria-expanded")).toBe("true");
});

it("folds what goes with a project's folder under the Trash option, counting it", async () => {
  mount({ kind: "delete-project", project }, [], [], {
    request: async () => ({ type: "project_removal_inspected", items: [UNCOMMITTED, UNMERGED] }),
  });
  checkAll();
  await act(async () => {});
  const along = disclosure("What goes with it (2)");
  expect(along.getAttribute("aria-expanded")).toBe("false");
  expect(document.getElementById(along.getAttribute("aria-describedby")!)?.textContent).toContain("Also move the project folder to the Trash");
  act(() => along.click());
  expect(disclosure("What goes with it (2)").getAttribute("aria-expanded")).toBe("true");
});

it("leaves what goes with a project's folder open when the locked option changes what it lists", async () => {
  mount({ kind: "delete-project", project }, [], [], {
    request: async () => ({ type: "project_removal_inspected", items: [UNCOMMITTED, LOCKED, PINNED] }),
  });
  checkAll();
  await act(async () => {});
  act(() => disclosure("What goes with it").click());
  act(() => boxes()[1].click());
  expect(disclosure("What goes with it (2)").getAttribute("aria-expanded")).toBe("true");
});

it("puts the one warning under the options, so a line it gains moves no checkbox, and says what moving the records does", async () => {
  const child = sessionOf("s-child", console_.id, project.id, "Child", "archived", { bound_to: owner.id });
  mount({ kind: "delete-session", session: { ...owner, status: "archived" } }, [owner, child], [], {
    request: async (body) => body.type === "inspect_session_branches"
      ? { type: "session_branches_inspected", items: [UNMERGED] }
      : { type: "session_records_inspected", presence: "present" },
  });
  await act(async () => {});
  const [warning] = warnings();
  for (const box of boxes()) expect(box.compareDocumentPosition(warning) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const tops = boxes().map((box) => box.closest("label"));
  act(() => boxes()[1].click());
  expect(boxes().map((box) => box.closest("label"))).toEqual(tops);
  expect(warningContent().textContent).toContain("Also moves the agent's own records of this session and of the sessions deleted with it to the Trash.");
});

it("describes the Trash option by the line saying more may go, while it shows", async () => {
  mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type === "inspect_project_removal") throw new Error("Connection lost");
      return { type: "ack" };
    },
  });
  checkAll();
  await act(async () => {});
  const ids = (checkbox().getAttribute("aria-describedby") ?? "").split(" ");
  expect(ids.map((id) => document.getElementById(id)?.textContent).join(" ")).toContain("deleted permanently");
  checkAll();
  expect((checkbox().getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ")).not.toContain("deleted permanently");
});

it("validates a project rename before requesting it, retaining excess Unicode text", async () => {
  const { daemon } = mount({ kind: "rename-project", project }, []);
  const field = document.querySelector<HTMLInputElement>("input")!;
  const setName = (value: string) => act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, value);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  const save = () => act(async () => document.querySelector<HTMLButtonElement>('button[type="submit"]')!.click());
  setName("😀".repeat(101));
  await save();
  expect(daemon.request).not.toHaveBeenCalled();
  expect(field.value).toBe("😀".repeat(101));
  expect(document.body.textContent).toContain("Name must be 100 characters or fewer.");
  setName("😀".repeat(100));
  await save();
  expect(daemon.request).toHaveBeenCalledWith({ type: "update_project", project: project.id, name: "😀".repeat(100) });
});
