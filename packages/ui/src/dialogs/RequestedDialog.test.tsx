// @vitest-environment jsdom
import { act, StrictMode } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, vi } from "vitest";

import { AFTER_FOCUS_MOVE_DELAY_MS } from "../components/StatusAnnouncer";
import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "../components/tooltipTestSupport";
import { accountOf, consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { errorAlerts } from "../errorAlerts";
import { isolate } from "../i18n/isolate";
import type { Account, Project, Session, SessionBranchItem } from "../protocol";
import { DaemonRequestError } from "../daemon-client";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import type { DialogRequest } from "./dialogRequest";
import { RequestedDialog } from "./RequestedDialog";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";
import { DEFAULT_SETTINGS } from "../defaultSettings";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const console_ = consoleOf("c-1", "Main");
const project = projectOf("p-1", console_.id, "Search API");
const owner = sessionOf("s-owner", console_.id, project.id, "Owner", "idle");
const account: Account = accountOf("a-1", "claude", "Work", "/home/dev/.claude-work");

/** Mounts the dialog for `dialog` over a store holding `sessions` and `accounts`. */
function mount(
  dialog: DialogRequest,
  sessions: Session[],
  accounts: Account[] = [],
  callbacks: { projects?: Project[]; onClose?: () => void; request?: Daemon["request"]; onSwitchAccounts?: (sessionIds: string[], account: string | null, accountName: string) => void; strict?: boolean } = {},
): { container: HTMLElement; daemon: Daemon } {
  const daemon: Daemon = {
    store: createStateStore({
      sessions: new Map(sessions.map((s) => [s.id, s])),
      projects: new Map((callbacks.projects ?? []).map((p) => [p.id, p])),
      settings: { ...DEFAULT_SETTINGS, auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts },
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
    <RequestedDialog dialog={dialog} onClose={callbacks.onClose ?? (() => {})} onSessionOpened={() => {}} onResumeSession={async () => {}} onSwitchAccounts={callbacks.onSwitchAccounts ?? (() => {})} onChangeSessionModel={() => {}} />
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

it("leaves the screen-reader-only name of the agent beside the icon as it was, set apart from the title by a comma", () => {
  const listed = sessionOf("s-2", console_.id, project.id, "Stopped", "interrupted", { bound_to: owner.id, account_id: account.id });
  mount({ kind: "archive-session", session: owner }, [owner, listed], [account]);
  const row = document.body.querySelector("[role=alertdialog] li")!;
  const name = row.querySelector(".sr-only")!;
  expect(name.textContent).toBe(`Claude Code (${isolate("Work")}), `);
  expect(name.childNodes).toHaveLength(1);
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


it.each([
  ["archive-session", "Archive session"],
  ["delete-session", "Delete session"],
  ["switch-account", "Switch account"],
] as const)("keeps the %s confirmation heading plain and names the session in its body as the terminal title changes", (kind, heading) => {
  const session = { ...owner, terminal_title: "First task" };
  const dialog: DialogRequest = kind === "switch-account"
    ? { kind, session, account: account.id, accountName: account.name }
    : { kind, session };
  const { daemon } = mount(dialog, [session]);
  const title = () => document.body.querySelector('[role="alertdialog"] h2')?.textContent;
  const body = () => document.body.querySelector('[role="alertdialog"] p')?.textContent;
  expect(title()).toBe(heading);
  expect(body()).toContain("First task");
  act(() => daemon.store.setState({ sessions: new Map([[session.id, { ...session, terminal_title: "Next task" }]]) }));
  expect(title()).toBe(heading);
  expect(body()).toContain(`${isolate("Next task")}｜${isolate(session.title)}`);
  expect(body()).not.toContain("First task");
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
    onClose, request: async () => ({ type: "ack", id: null }),
  });
  const titles = () => [...document.querySelectorAll("[role=alertdialog] li")].map((row) => row.textContent);
  expect(titles()).toHaveLength(3);
  for (const title of [manual.title, lead.title, nested.title]) expect(titles().some((text) => text?.includes(title))).toBe(true);
  expect(sent(daemon, "archive_session")).toEqual([]);
  const late = sessionOf("late", console_.id, project.id, "New session", "starting", { bound_to: hub.id });
  act(() => daemon.store.setState({ sessions: new Map([...daemon.store.getState().sessions, [late.id, late]]) }));
  expect(titles()).toHaveLength(4);
  expect(document.querySelector("[role=alertdialog]")?.textContent).toContain("Archived with it (4)");
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
      if (body.type === "inspect_session_branches") return { type: "session_branches_inspected", id: null, items: [] };
      if (body.type === "inspect_session_records") return { type: "session_records_inspected", id: null, presence: "present" };
      if (body.type === "inspect_project_removal") return { type: "project_removal_inspected", id: null, items: [], processes: [] };
      if (body.type === "list_processes") return { type: "processes_listed", id: null, trees: [] };
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
      ? { type: "session_branches_inspected", id: null, items: [UNCOMMITTED, UNMERGED] }
      : { type: "session_records_inspected", id: null, presence: "absent" },
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
    request: async () => ({ type: "session_branches_inspected", id: null, items: [UNCOMMITTED, UNMERGED] }),
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
    request: async () => ({ type: "session_branches_inspected", id: null, items: [LOCKED] }),
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
  const unused = { ...owner, agent, has_conversation: false, agent_session_id: agent === "codex" ? null : owner.agent_session_id };
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose,
    strict: true,
    request: async () => ({ type: "ack", id: null }),
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
    onClose, request: async () => ({ type: "ack", id: null }),
  });
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledExactlyOnceWith({ type: "delete_session", session: unused.id, only_if_unused: true });
  expect(onClose).toHaveBeenCalledOnce();
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
});

it("keeps the confirmation for a used archived session but hides cleanup when no files exist", async () => {
  const archived = { ...owner, status: "archived" as const };
  mount({ kind: "delete-session", session: archived }, [archived], [], {
    request: async () => ({ type: "session_records_inspected", id: null, presence: "absent" }),
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
    request: async () => ({ type: "session_records_inspected", id: null, presence: "present" }),
  });
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledWith({ type: "inspect_session_records", sessions: [child.id, owner.id].sort() });
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).not.toBeNull();
});


it("keeps the bulk deletion confirmation but removes cleanup for a file-free scope", async () => {
  const archived = { ...owner, status: "archived" as const };
  const { daemon } = mount({ kind: "delete-archived", console: console_, project, count: 1 }, [archived], [], {
    request: async () => ({ type: "session_records_inspected", id: null, presence: "absent" }),
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
  let resolve!: (value: { type: "session_records_inspected"; id: null; presence: "absent" }) => void;
  const waiting = new Promise<{ type: "session_records_inspected"; id: null; presence: "absent" }>((done) => { resolve = done; });
  mount(kind === "single"
    ? { kind: "delete-session", session: archived }
    : { kind: "delete-archived", console: console_, project, count: 1 }, [archived], [], {
    request: () => waiting,
  });
  const option = checkbox();
  act(() => option.focus());
  expect(document.activeElement).toBe(option);
  await act(async () => { resolve({ type: "session_records_inspected", id: null, presence: "absent" }); });
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
      if (body.type === "delete_session") { await waiting.promise; return { type: "ack", id: null }; }
      return { type: "session_records_inspected", id: null, presence: "absent" };
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
      ? { type: "session_branches_inspected", id: null, items: [UNCOMMITTED, UNMERGED, KEPT] }
      : { type: "session_records_inspected", id: null, presence: "absent" },
  });
  await act(async () => {});
  // Only an archive asks for the processes it would stop.
  expect(sent(daemon, "inspect_session_branches")).toEqual([{ type: "inspect_session_branches", sessions: [owner.id], ...(dialog.kind === "archive-session" ? { processes: true } : {}) }]);
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

const TREE = {
  id: "4242:1", session_id: owner.id, command: "npm run dev", cwd: "/r-feature", cwd_missing: false, kept: false,
  started_at: 1_000, process_count: 1, memory_bytes: 1_000_000, ports: [],
};

it("lists the processes an archive stops, stops those ticked by default and keeps only unchecked ones running, and does not ask again for an inspection in hand", async () => {
  const { daemon } = mount({ kind: "archive-session", session: owner, inspection: { sessions: [owner.id], items: [], processes: [TREE, { ...TREE, id: "4300:2", command: "tail -f log" }] } }, [owner]);
  await act(async () => {});
  expect(sent(daemon, "inspect_session_branches")).toEqual([]);
  expect(document.querySelector("[role=alertdialog]")?.textContent).toContain("2 processes are stopped.");
  expect(boxes().map((box) => box.checked)).toEqual([true, true]);
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "archive_session", session: owner.id });
  act(() => boxes()[1].click());
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "archive_session", session: owner.id, keep_process_ids: ["4300:2"] });
  act(() => boxes()[0].click());
  expect(document.querySelector("[role=alertdialog]")?.textContent).not.toContain("selected process");
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "archive_session", session: owner.id, keep_process_ids: [TREE.id, "4300:2"] });
  act(() => boxes()[1].click());
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "archive_session", session: owner.id, keep_process_ids: [TREE.id] });
});

it("heads each topic of an archive as a labelled group, with a divider only between the groups shown and a group's consequences inside it", async () => {
  const child = sessionOf("s-child", console_.id, project.id, "Child", "idle", { bound_to: owner.id });
  mount({ kind: "archive-session", session: owner, inspection: { sessions: [owner.id, child.id], items: [UNMERGED], processes: [TREE] } }, [owner, child]);
  await act(async () => {});
  act(() => boxes()[0].click());
  const dialog = document.querySelector("[role=alertdialog]")!;
  const groups = [...dialog.querySelectorAll("[role=group]")].filter((group) => group.querySelector(":scope > h3"));
  expect(groups.map((group) => group.querySelector("h3")?.textContent)).toEqual(["Archived with it (1)", "Worktrees and branches", "Background processes"]);
  expect(dialog.querySelectorAll(".groove-t")).toHaveLength(2);
  expect(groups[1].textContent).toContain("Also permanently deletes");
  expect(groups[2].textContent).toContain("1 process is stopped.");
});

it("lists a console session's trees as rows that are always stopped, with no checkbox, and counts them with the ticked ones", async () => {
  const consoleSession = sessionOf("s-console", console_.id, undefined, "Main", "idle");
  const consoleTree = { ...TREE, id: "4400:3", session_id: consoleSession.id, command: "node console.js" };
  const bound = { ...owner, bound_to: consoleSession.id };
  const { daemon } = mount({ kind: "archive-session", session: consoleSession, inspection: { sessions: [consoleSession.id, bound.id], items: [], processes: [consoleTree, TREE] } }, [consoleSession, bound]);
  await act(async () => {});
  const dialog = document.querySelector("[role=alertdialog]")!;
  expect(boxes()).toHaveLength(1);
  expect(dialog.textContent).toContain("A console session's processes are always stopped.");
  expect(dialog.textContent).toContain("node console.js");
  expect(dialog.textContent).toContain("2 processes are stopped.");
  act(() => boxes()[0].click());
  expect(dialog.textContent).toContain("1 process is stopped.");
  expect(dialog.textContent).toContain("The process you keep stays running.");
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "archive_session", session: consoleSession.id, keep_process_ids: [TREE.id] });
});

it("offers no checkbox at all when every process of an archive is a console session's", async () => {
  const consoleSession = sessionOf("s-console", console_.id, undefined, "Main", "idle");
  mount({ kind: "archive-session", session: consoleSession, inspection: { sessions: [consoleSession.id], items: [], processes: [{ ...TREE, session_id: consoleSession.id }] } }, [consoleSession]);
  await act(async () => {});
  expect(boxes()).toHaveLength(0);
  const dialog = document.querySelector("[role=alertdialog]")!;
  expect(dialog.textContent).not.toContain("Uncheck any");
  expect(dialog.textContent).toContain("1 process is stopped.");
});

it("shows no section, and no divider, for an archive with nothing to list", async () => {
  mount({ kind: "archive-session", session: owner, inspection: { sessions: [owner.id], items: [], processes: [TREE] } }, [owner]);
  await act(async () => {});
  const dialog = document.querySelector("[role=alertdialog]")!;
  expect([...dialog.querySelectorAll("h3")].map((h) => h.textContent)).toEqual(["Background processes"]);
  expect(dialog.querySelector(".groove-t")).toBeNull();
});

it("preserves process choices through another inspection and selects newly found trees for stopping", async () => {
  const late = sessionOf("s-late", console_.id, project.id, "Late", "idle", { bound_to: owner.id });
  const answer = deferred();
  const { daemon } = mount({ kind: "archive-session", session: owner, inspection: { sessions: [owner.id], items: [], processes: [TREE] } }, [owner], [], {
    request: async (body) => {
      if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
      await answer.promise;
      return { type: "session_branches_inspected", id: null, items: [], processes: [TREE, { ...TREE, id: "4300:2", command: "tail -f log" }] };
    },
  });
  await act(async () => {});
  act(() => boxes()[0].click());
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, owner], [late.id, late]]) }));
  expect(boxes()[0].checked).toBe(false);
  await act(async () => answer.resolve());
  expect(boxes().map((box) => box.checked)).toEqual([false, true]);
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "archive_session", session: owner.id, keep_process_ids: [TREE.id] });
});

it.each(["rejection", "unexpected reply", "empty success"] as const)("preserves only in-scope keep choices after a reinspection %s, while a successful empty result clears them", async (outcome) => {
  const child = sessionOf("s-child", console_.id, project.id, "Child", "idle", { bound_to: owner.id });
  const childTree = { ...TREE, id: "4300:2", session_id: child.id, command: "node child.js" };
  const joined = sessionOf("s-joined", console_.id, project.id, "Joined", "idle", { bound_to: owner.id });
  const answer = deferred();
  const { daemon } = mount({ kind: "archive-session", session: owner, inspection: { sessions: [owner.id, child.id], items: [UNMERGED], processes: [TREE, childTree] } }, [owner, child], [], {
    request: async (body) => {
      if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
      await answer.promise;
      if (outcome === "rejection") throw new Error("Inspection unavailable");
      if (outcome === "unexpected reply") return { type: "ack", id: null };
      return { type: "session_branches_inspected", id: null, items: [], processes: [] };
    },
  });
  await act(async () => {});
  act(() => boxes().forEach((box) => box.click()));
  expect(boxes().map((box) => box.checked)).toEqual([true, false, false]);
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, owner], [joined.id, joined]]) }));
  await act(async () => answer.resolve());
  expect(sent(daemon, "inspect_session_branches")).toEqual([{ type: "inspect_session_branches", sessions: [joined.id, owner.id].sort(), processes: true }]);
  const dialog = document.querySelector("[role=alertdialog]")!;
  expect(dialog.querySelector('[data-pending="true"]')).toBeNull();
  expect(dialog.textContent).not.toContain("node child.js");
  expect(boxes().map((box) => box.checked)).toEqual(outcome === "empty success" ? [] : [false]);
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({
    type: "archive_session", session: owner.id,
    ...(outcome === "empty success" ? {} : { keep_process_ids: [TREE.id] }),
  });
});

it("states the consequences in the order of the controls, the branch cleanup first and the processes after", async () => {
  mount({ kind: "archive-session", session: owner, inspection: { sessions: [owner.id], items: [UNCOMMITTED, UNMERGED], processes: [TREE, { ...TREE, id: "4300:2", command: "tail -f log" }] } }, [owner]);
  await act(async () => {});
  expect(boxes()).toHaveLength(3);
  act(() => boxes()[0].click());
  act(() => boxes()[2].click());
  const text = document.querySelector("[role=alertdialog]")!.textContent!;
  const forced = text.indexOf("permanently deletes");
  const stopped = text.indexOf("1 process is stopped.");
  const kept = text.indexOf("The process you keep stays running");
  expect(forced).toBeGreaterThan(-1);
  expect(stopped).toBeGreaterThan(forced);
  expect(kept).toBeGreaterThan(stopped);
});

it("keeps a process's working directory and facts, not its command, in a Details disclosure under its checkbox, collapsed until opened", async () => {
  const command = "node server.js ".concat("--flag=value ".repeat(20)).trim();
  mount({ kind: "archive-session", session: owner, inspection: { sessions: [owner.id], items: [], processes: [{ ...TREE, command }] } }, [owner]);
  await act(async () => {});
  const dialog = document.querySelector("[role=alertdialog]")!;
  const toggle = dialog.querySelector<HTMLButtonElement>("button[aria-expanded]")!;
  expect(toggle.textContent).toBe("Details");
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(toggle.getAttribute("aria-describedby")).toBeTruthy();
  expect(document.getElementById(toggle.getAttribute("aria-describedby")!)?.contains(boxes()[0])).toBe(true);
  // No custom tooltip or description for assistive technology stands in for it.
  expect(boxes()[0].getAttribute("aria-describedby")).toBeNull();
  act(() => toggle.click());
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  const text = dialog.textContent!.replace(/[\u2066-\u2069]/g, "");
  expect(text).toContain(command);
  const details = document.getElementById(toggle.getAttribute("aria-controls")!)!.textContent!.replace(/[\u2066-\u2069]/g, "");
  expect(details).toContain("/r-feature");
  expect(details).toContain("1 MB of memory");
  expect(details).not.toContain(command);
});

it("offers no choice of processes when the archive finds none", async () => {
  const { daemon } = mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", id: null, items: [] }),
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).toBeNull();
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "archive_session", session: owner.id });
});

it("offers nothing to force when everything bound can go or has to stay", async () => {
  mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", id: null, items: [{ ...UNMERGED, state: "removable" }, KEPT] }),
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).toBeNull();
});

it("lists locked worktrees apart, with their changes, under a checkbox of their own that forces them alone", async () => {
  const { daemon } = mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", id: null, items: [UNMERGED, { ...LOCKED, uncommitted: true }] }),
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
      if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
      if (body.sessions.length === 1) return { type: "session_branches_inspected", id: null, items: [UNMERGED] };
      await second.promise;
      return { type: "session_branches_inspected", id: null, items: [UNMERGED, { ...UNMERGED, branch: "late" }] };
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
      if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
      if (body.sessions.length === 1) {
        await first.promise;
        return { type: "session_branches_inspected", id: null, items: [{ ...UNMERGED, branch: "stale" }] };
      }
      return { type: "session_branches_inspected", id: null, items: [UNMERGED] };
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
    request: async () => ({ type: "project_removal_inspected", id: null, items: [UNCOMMITTED, UNMERGED], processes: [] }),
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

/** The confirmation's section headed "Project processes", if shown. */
const projectProcessesGroup = () => [...document.querySelectorAll("[role=alertdialog] [role=group]")].find((candidate) => candidate.querySelector(":scope > h3")?.textContent === "Project processes") ?? null;

it("lists the project processes a removal stops from the project's process list as rows under their own heading, and shows no section when there are none", async () => {
  const kept = { ...TREE, session_id: undefined, kept: true };
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => body.type === "list_processes" ? { type: "processes_listed", id: null, trees: [kept, { ...TREE, id: "4300:2", session_id: undefined }, TREE] } : { type: "ack", id: null },
  });
  await act(async () => {});
  expect(sent(daemon, "list_processes")).toEqual([{ type: "list_processes", project_id: project.id }]);
  expect(sent(daemon, "inspect_project_removal")).toEqual([]);
  const group = [...document.querySelectorAll("[role=group]")].find((candidate) => candidate.querySelector(":scope > h3")?.textContent === "Project processes")!;
  expect(group.querySelectorAll("button[aria-expanded]")).toHaveLength(2);
  expect(group.textContent).toContain("2 processes are stopped.");
  expect(group.querySelector("input[type=checkbox]")).toBeNull();
});

it("lists the project processes a console's deletion stops, one request per project of the console, and none of another console's", async () => {
  const second = projectOf("p-2", console_.id, "Docs");
  const elsewhere = projectOf("p-3", "c-2", "Other");
  const stray = { ...TREE, id: "4300:2", session_id: undefined };
  const { daemon } = mount({ kind: "delete-console", console: console_ }, [], [], {
    projects: [project, second, elsewhere],
    request: async (body) => body.type === "list_processes" ? { type: "processes_listed", id: null, trees: "project_id" in body && body.project_id === second.id ? [{ ...stray, id: "4300:9" }, stray] : [stray, TREE] } : { type: "ack", id: null },
  });
  await act(async () => {});
  expect((sent(daemon, "list_processes") as { project_id: string }[]).map((body) => body.project_id).sort()).toEqual(["p-1", "p-2"]);
  const dialog = document.querySelector("[role=alertdialog]")!;
  const group = [...dialog.querySelectorAll("[role=group]")].find((candidate) => candidate.querySelector(":scope > h3")?.textContent === "Project processes")!;
  expect(group.querySelectorAll("button[aria-expanded]")).toHaveLength(3);
  expect(group.textContent).toContain("3 processes are stopped.");
  expect(dialog.querySelectorAll(".groove-t")).toHaveLength(0);
});

it("shows no project processes section in a console's deletion when none of its projects has any", async () => {
  mount({ kind: "delete-console", console: console_ }, [], [], {
    projects: [project, projectOf("p-2", console_.id, "Docs")],
    request: async (body) => body.type === "list_processes" ? { type: "processes_listed", id: null, trees: [TREE] } : { type: "ack", id: null },
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog] [role=group]")).toBeNull();
});

it("says in a console's deletion that a project's processes could not be listed, and still lists those of the others", async () => {
  const stray = { ...TREE, id: "4300:2", session_id: undefined };
  mount({ kind: "delete-console", console: console_ }, [], [], {
    projects: [project, projectOf("p-2", console_.id, "Docs")],
    request: async (body) => {
      if (body.type !== "list_processes") return { type: "ack", id: null };
      if ("project_id" in body && body.project_id === "p-2") throw new Error("gone");
      return { type: "processes_listed", id: null, trees: [stray] };
    },
  });
  await act(async () => {});
  const group = projectProcessesGroup()!;
  expect(group.textContent).toContain("Could not list all the project processes");
  expect(group.textContent).toContain("Any that are running will still be stopped.");
  expect(group.textContent).not.toContain("is stopped");
});

it("says in a project's removal that its project processes could not be listed, in place of the section reading as none", async () => {
  mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => { if (body.type === "list_processes") throw new Error("gone"); return { type: "ack", id: null }; },
  });
  await act(async () => {});
  const group = projectProcessesGroup()!;
  expect(group.querySelector("[role=alert], [data-slot=alert-title]")).not.toBeNull();
  expect(group.textContent).toContain("Could not list the project processes");
  expect(group.textContent).not.toContain("are stopped");
  expect(group.querySelector(".font-mono")).toBeNull();
});

it("announces the failed project process list once the dialog's focus has settled, not as it mounts", async () => {
  const settle = withFakeTimers();
  mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => { if (body.type === "list_processes") throw new Error("gone"); return { type: "ack", id: null }; },
  });
  await act(async () => {});
  const group = projectProcessesGroup()!;
  expect(group.querySelector("[role=alert]")).toBeNull();
  await act(async () => void vi.advanceTimersByTime(AFTER_FOCUS_MOVE_DELAY_MS));
  expect(group.querySelector("[role=alert]")?.textContent).toContain("Could not list the project processes");
  settle();
});

it.each<[string, DialogRequest]>([
  ["a project's removal", { kind: "delete-project", project }],
  ["a console's deletion", { kind: "delete-console", console: console_ }],
])("holds %s until its project processes have been listed", async (_name, request) => {
  const answer = deferred();
  const { daemon } = mount(request, [], [], {
    projects: [project],
    request: async (body) => {
      if (body.type !== "list_processes") return { type: "ack", id: null };
      await answer.promise;
      return { type: "processes_listed", id: null, trees: [{ ...TREE, session_id: undefined }] };
    },
  });
  await act(async () => document.body.querySelector("[role=alertdialog] form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(sent(daemon, "delete_project")).toEqual([]);
  expect(sent(daemon, "delete_console")).toEqual([]);
  await act(async () => answer.resolve());
  expect(projectProcessesGroup()).not.toBeNull();
});

it("heads a project's removal by its running sessions, its folder and its project processes, a divider only between the sections shown", async () => {
  const running = { ...owner, status: "working" as const };
  mount({ kind: "delete-project", project }, [running], [], {
    request: async (body) => body.type === "list_processes" ? { type: "processes_listed", id: null, trees: [{ ...TREE, session_id: undefined }] } : { type: "ack", id: null },
  });
  await act(async () => {});
  const dialog = document.querySelector("[role=alertdialog]")!;
  const headings = () => [...dialog.querySelectorAll("[role=group] > h3")].map((h) => h.textContent);
  expect(headings()).toEqual(["Sessions that will be ended (1)", "Project folder", "Project processes"]);
  expect(dialog.querySelectorAll(".groove-t")).toHaveLength(2);
  expect(dialog.textContent).not.toContain("will also be stopped");
});

it("shows only the project folder section for a project with nothing running", async () => {
  mount({ kind: "delete-project", project }, [], [], { request: async () => ({ type: "processes_listed", id: null, trees: [] }) });
  await act(async () => {});
  const dialog = document.querySelector("[role=alertdialog]")!;
  expect([...dialog.querySelectorAll("[role=group] > h3")].map((h) => h.textContent)).toEqual(["Project folder"]);
  expect(dialog.querySelectorAll(".groove-t")).toHaveLength(0);
});

it("shows no Project processes section when the project has none", async () => {
  mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => body.type === "list_processes" ? { type: "processes_listed", id: null, trees: [TREE] } : { type: "ack", id: null },
  });
  await act(async () => {});
  expect(projectProcessesGroup()).toBeNull();
});

it("holds a project's removal while what goes along is asked, and says more may go when that fails", async () => {
  const answer = deferred();
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type !== "inspect_project_removal") return { type: "ack", id: null };
      await answer.promise;
      throw new Error("Connection lost");
    },
  });
  checkAll();
  await act(async () => document.body.querySelector("[role=alertdialog] form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(sent(daemon, "delete_project")).toEqual([]);
  await act(async () => answer.resolve());
  await act(async () => unknownMark()!.click());
  expect(document.querySelector("[data-status-mark-popover]")?.textContent).toContain("its branches deleted permanently");
  await submitDialog();
  expect(sent(daemon, "delete_project")).toHaveLength(1);
});

it("takes a locked worktree, and the branch it has checked out, along with a project only when chosen", async () => {
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async () => ({ type: "project_removal_inspected", id: null, items: [UNCOMMITTED, LOCKED, PINNED], processes: [] }),
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

it("lists the worktrees an inspection held back as staying, and sends back its token, only with the folder", async () => {
  const held = { ...UNCOMMITTED, path: "/r-held", branch: "held", state: "failed" as const, uncommitted: false, detail: "could not be told", cause: "project_folder_unknown" as const };
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => body.type === "inspect_project_removal"
      ? { type: "project_removal_inspected", id: null, items: [UNCOMMITTED, held], processes: [], inspection: "token-2" }
      : { type: "ack", id: null },
  });
  checkAll();
  await act(async () => {});
  expect(listed()).toEqual(["/r-feature on branch feature", "/r-held on branch held"]);
  const dialog = document.querySelector("[role=alertdialog]")!;
  expect(dialog.textContent).toContain("What goes with it");
  expect(dialog.textContent).toContain("What stays");
  expect(dialog.textContent).toContain("This worktree stays, as whether it is or holds another project's folder could not be checked:");
  // Only the worktree that goes is counted among what goes.
  expect(dialog.textContent).toContain("Also moves the worktree outside the folder to the Trash.");
  expect(dialog.textContent).not.toContain("could not be told");
  await submitDialog();
  expect(sent(daemon, "delete_project")).toEqual([expect.objectContaining({ trash_directory: true, inspection: "token-2" })]);
  act(() => boxes()[0].click());
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "delete_project", project: project.id, stop_sessions: false, trash_directory: false });
});

it("asks again what goes along once a project's removal with its folder is refused, holding Confirm for it, so that a retry carries out what is shown", async () => {
  const refusal = { path: "/work/p", detail: "it did not respond in time", cause: "unanswered" };
  const second = deferred();
  let inspections = 0;
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type === "inspect_project_removal") {
        inspections += 1;
        if (inspections === 1) throw new DaemonRequestError("refused", "project_directory_trash_failed", { ...refusal, inspection: "token-1" });
        await second.promise;
        return { type: "project_removal_inspected", id: null, items: [UNCOMMITTED], processes: [] };
      }
      if (body.type === "delete_project" && body.inspection) throw new DaemonRequestError("refused", "project_directory_trash_failed", refusal);
      return { type: "ack", id: null };
    },
  });
  try {
    checkAll();
    await act(async () => {});
    expect(listed()).toEqual([]);
    await submitDialog();
    expect(sent(daemon, "delete_project")).toEqual([expect.objectContaining({ trash_directory: true, inspection: "token-1" })]);
    expect(errorAlerts.getSnapshot()).toHaveLength(1);
    expect(sent(daemon, "inspect_project_removal")).toHaveLength(2);
    // Confirm waits on the new answer.
    await submitDialog();
    expect(sent(daemon, "delete_project")).toHaveLength(1);
    await act(async () => second.resolve());
    expect(listed()).toEqual(["/r-feature on branch feature"]);
    await submitDialog();
    const deletes = sent(daemon, "delete_project");
    expect(deletes).toHaveLength(2);
    expect(deletes[1]).toEqual({ type: "delete_project", project: project.id, stop_sessions: false, trash_directory: true });
  } finally {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  }
});

/** The mark after the Trash option's label that says what else may go, when the lookup failed. */
const unknownMark = () => document.querySelector<HTMLElement>("[role=alertdialog] button[data-status-mark][aria-label='Other worktrees unknown']");
/** The list of sessions the confirmation says it takes along. */
const sessionList = (text: string) => [...document.querySelectorAll("[role=alertdialog] ul")].find((ul) => ul.textContent?.includes(text))!;
const dialogParagraph = (text: string) => [...document.querySelectorAll("[role=alertdialog] p")].find((p) => p.textContent?.includes(text));

it("marks that more may go along with a project beside the Trash option, outside its label, with no alert", async () => {
  mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type === "inspect_project_removal") throw new Error("Connection lost");
      return body.type === "list_processes" ? { type: "processes_listed", id: null, trees: [] } : { type: "ack", id: null };
    },
  });
  expect(unknownMark()).toBeNull();
  checkAll();
  await act(async () => {});
  const mark = unknownMark()!;
  // The checkbox is named by its label alone; the mark is no content of that label.
  expect(mark.closest("label")).toBeNull();
  expect(checkbox().labels?.[0]?.textContent).toBe("Also move the project folder to the Trash");
  expect(document.querySelector("[role=alertdialog] [data-slot=alert-root]")).toBeNull();
  expect(dialogParagraph("deleted permanently")).toBeUndefined();
  // The mark's press leaves the option ticked.
  await act(async () => mark.click());
  expect(checkbox().checked).toBe(true);
  checkAll();
  expect(unknownMark()).toBeNull();
});

it("adds what a ticked option does as body text after the list, which holds no checkbox", async () => {
  const child = sessionOf("s-child", console_.id, project.id, "Child", "archived", { bound_to: owner.id });
  mount({ kind: "delete-session", session: { ...owner, status: "archived" } }, [owner, child], [], {
    request: async (body) => body.type === "inspect_session_branches"
      ? { type: "session_branches_inspected", id: null, items: [UNCOMMITTED, UNMERGED, LOCKED] }
      : { type: "session_records_inspected", id: null, presence: "absent" },
  });
  await act(async () => {});
  const dialog = document.querySelector("[role=alertdialog]")!;
  expect(dialog.querySelector("[data-slot=alert-root]")).toBeNull();
  // The sentence that introduces the list, then the list itself, as plain body text.
  const list = sessionList("Child");
  expect(list.parentElement!.querySelector("input[type=checkbox]")).toBeNull();
  expect(list.previousElementSibling?.tagName).toBe("P");
  act(() => boxes()[0].click());
  act(() => boxes()[1].click());
  const forced = dialogParagraph("Also permanently deletes 2 worktrees and branches Git keeps.")!;
  expect(forced).toBeDefined();
  // The list comes first and the lines after it, in the same block.
  expect(list.compareDocumentPosition(forced) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(list.parentElement).toBe(forced.parentElement!.parentElement);
  const described = boxes().flatMap((box) => (box.getAttribute("aria-describedby") ?? "").split(" ").filter(Boolean));
  for (const id of described) expect(document.getElementById(id)).not.toBeNull();
  expect(dialogParagraph("Also permanently deletes the locked worktree, uncommitted changes included.")).toBeDefined();
  act(() => boxes()[0].click());
  expect(dialogParagraph("worktrees and branches Git keeps")).toBeUndefined();
});

it("shows consequence text only once an option is ticked where the confirmation has none of its own", async () => {
  mount({ kind: "archive-session", session: owner }, [owner], [], {
    request: async () => ({ type: "session_branches_inspected", id: null, items: [UNMERGED] }),
  });
  await act(async () => {});
  expect(dialogParagraph("Also permanently deletes")).toBeUndefined();
  checkAll();
  expect(dialogParagraph("Also permanently deletes")).toBeDefined();
  expect(document.querySelector("[role=alertdialog] [data-slot=alert-root]")).toBeNull();
  expect(document.querySelector("[role=alertdialog]")!.textContent).not.toContain("Additional consequences");
  checkAll();
  expect(dialogParagraph("Also permanently deletes")).toBeUndefined();
});

it.each(CONFIRMATIONS)("is a warning modal when %s: the warning icon in the header, the list as body text, the danger action trailing", (_, dialog, listed) => {
  // An earlier test's `click()` leaves react-aria's modality on virtual, where the dialog's opening focus waits a frame.
  act(() => setInteractionModality("pointer"));
  mount(dialog, [owner, listed]);
  const modal = document.querySelector("[role=alertdialog]")!;
  expect(modal.querySelector("[data-slot=alert-dialog-header] [data-slot=alert-dialog-icon]")).not.toBeNull();
  expect(modal.querySelector("[data-slot=alert-root]")).toBeNull();
  expect(sessionList(listed.title ?? "")?.closest("[data-slot=alert-dialog-body]")).not.toBeNull();
  // Destructive confirmations keep focus on the dialog, and the secondary comes before the primary.
  if (modal.querySelector("button[type=submit]")?.className.includes("danger")) expect(document.activeElement?.matches("[role=alertdialog]")).toBe(true);
  const buttons = [...modal.querySelectorAll("[data-slot=alert-dialog-footer] button")];
  expect(buttons.map((button) => button.textContent)).toEqual(["Cancel", confirmText()]);
  expect(buttons[1]).toBe(confirmButton());
});

it("forces no locked worktree once the Trash option is unticked after the locked one was ticked", async () => {
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async () => ({ type: "project_removal_inspected", id: null, items: [LOCKED], processes: [] }),
  });
  checkAll();
  await act(async () => {});
  act(() => boxes()[1].click());
  expect(boxes()[1].checked).toBe(true);
  act(() => boxes()[0].click());
  await submitDialog();
  expect(vi.mocked(daemon.request)).toHaveBeenLastCalledWith({ type: "delete_project", project: project.id, stop_sessions: false, trash_directory: false });
});

it.each([
  ["project_directory_shared", { project: "p-2" }],
  ["project_directory_protected", { path: "/work/p" }],
  ["project_directory_trash_failed", { path: "/work/p", detail: "it could not be looked at" }],
])("warns of nothing going along with a project whose removal would be refused with %s", async (code, params) => {
  const { daemon } = mount({ kind: "delete-project", project }, [], [], {
    request: async (body) => {
      if (body.type === "inspect_project_removal") throw new DaemonRequestError("refused", code, params);
      return { type: "ack", id: null };
    },
  });
  checkAll();
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")?.textContent).not.toContain("deleted permanently");
  expect(listed()).toEqual([]);
  expect(unknownMark()).toBeNull();
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
      if (body.type !== "inspect_session_branches") return { type: "ack", id: null };
      return { type: "session_branches_inspected", id: null, items: body.sessions.length === 1 ? [UNMERGED, LOCKED] : [UNMERGED, LOCKED, { ...UNMERGED, branch: "late" }] };
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
    request: async () => ({ type: "project_removal_inspected", id: null, items: [UNCOMMITTED, UNMERGED], processes: [] }),
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
    request: async () => ({ type: "project_removal_inspected", id: null, items: [UNCOMMITTED, LOCKED, PINNED], processes: [] }),
  });
  checkAll();
  await act(async () => {});
  act(() => disclosure("What goes with it").click());
  act(() => boxes()[1].click());
  expect(disclosure("What goes with it (2)").getAttribute("aria-expanded")).toBe("true");
});

it("puts the consequences under the options, so a line they gain moves no checkbox, and says what moving the records does", async () => {
  const child = sessionOf("s-child", console_.id, project.id, "Child", "archived", { bound_to: owner.id });
  mount({ kind: "delete-session", session: { ...owner, status: "archived" } }, [owner, child], [], {
    request: async (body) => body.type === "inspect_session_branches"
      ? { type: "session_branches_inspected", id: null, items: [UNMERGED] }
      : { type: "session_records_inspected", id: null, presence: "present" },
  });
  await act(async () => {});
  const list = sessionList("Child");
  for (const box of boxes()) expect(box.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const tops = boxes().map((box) => box.closest("label"));
  act(() => boxes()[1].click());
  expect(boxes().map((box) => box.closest("label"))).toEqual(tops);
  expect(dialogParagraph("Also moves the agent's own records of this session and of the sessions deleted with it to the Trash.")).toBeDefined();
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

const unbindOwner = sessionOf("s-unbind-owner", console_.id, undefined, "Hub", "idle", { role: "console" });
const unbindable = sessionOf("s-unbindable", console_.id, project.id, "Worker", "idle", { bound_to: unbindOwner.id });
const cancelButton = () => document.body.querySelector("[data-slot=alert-dialog-footer] button:first-child");

it("names the session and its owner in the unbind confirmation and releases only on confirm", async () => {
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable], [], { onClose, request: async () => ({ type: "ack", id: null }) });
  const text = document.querySelector("[role=alertdialog]")!.textContent!.replace(/[\u2066-\u2069]/g, "");
  expect(text).toContain("Unbind session");
  expect(text).toContain("Worker will no longer report to Hub");
  expect(cancelButton()?.textContent).toBe("Cancel");
  expect(confirmText()).toBe("Unbind");
  expect(daemon.request).not.toHaveBeenCalled();
  await submitDialog();
  expect(daemon.request).toHaveBeenCalledExactlyOnceWith({ type: "release_session", session: unbindable.id });
  expect(onClose).toHaveBeenCalledOnce();
  expect(daemon.toastError).not.toHaveBeenCalled();
});

it("is a warning confirmation when unbinding: the warning icon leads, the action is primary, the consequence is shown and each name appears once", () => {
  mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable]);
  const modal = document.querySelector("[role=alertdialog]")!;
  const header = modal.querySelector("[data-slot=alert-dialog-header]")!;
  expect(header.firstElementChild?.matches("[data-slot=alert-dialog-icon]")).toBe(true);
  expect(confirmButton()!.className).not.toContain("danger");
  const body = modal.querySelector("[data-slot=alert-dialog-body]")!.textContent!.replace(/[\u2066-\u2069]/g, "");
  expect(body).toContain("The owner will no longer be able to instruct it");
  expect(body.split("Worker")).toHaveLength(2);
  expect(body.split("Hub")).toHaveLength(2);
});

it("releases nothing when the unbind confirmation is cancelled", () => {
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable], [], { onClose });
  act(() => (cancelButton() as HTMLButtonElement).click());
  expect(onClose).toHaveBeenCalledOnce();
  expect(daemon.request).not.toHaveBeenCalled();
});

it("keeps the unbind confirmation open when the release fails unexpectedly", async () => {
  const onClose = vi.fn();
  try {
    const { daemon } = mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable], [], {
      onClose,
      request: async () => { throw new DaemonRequestError("internal error", "internal_error", {}); },
    });
    await submitDialog();
    expect(onClose).not.toHaveBeenCalled();
    expect(daemon.request).toHaveBeenCalledOnce();
    expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
    expect(errorAlerts.getSnapshot()).toHaveLength(1);
  } finally {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  }
});

it("closes the unbind confirmation silently when the daemon refuses with session_not_bound", async () => {
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable], [], {
    onClose,
    request: async () => { throw new DaemonRequestError("`s-unbindable` is not bound to any owner, so there is nothing to release", "session_not_bound", { session: unbindable.id }); },
  });
  await submitDialog();
  expect(onClose).toHaveBeenCalledOnce();
  expect(daemon.toastError).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it("shows session_binding_changed and keeps the unbind confirmation open, naming the new owner once the store has it", async () => {
  const onClose = vi.fn();
  const newOwner = sessionOf("s-new-owner", console_.id, undefined, "Other hub", "idle", { role: "console" });
  try {
    const { daemon } = mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, newOwner, unbindable], [], {
      onClose,
      request: async () => { throw new DaemonRequestError("raw daemon text", "session_binding_changed", { session: unbindable.id }); },
    });
    await submitDialog();
    expect(onClose).not.toHaveBeenCalled();
    const [alert] = errorAlerts.getSnapshot();
    expect(alert.error).toMatchObject({ code: "session_binding_changed" });
    expect(alert.title).toBe("Could not unbind the session");
    expect(alert.classification).toEqual({ kind: "actionable", recovery: "ownerMoved" });
    await act(async () => {
      daemon.store.setState({ sessions: new Map([unbindOwner, newOwner, { ...unbindable, bound_to: newOwner.id }].map((s) => [s.id, s])) });
    });
    expect(document.querySelector("[role=alertdialog]")!.textContent).toContain("Other hub");
    expect(onClose).not.toHaveBeenCalled();
  } finally {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  }
});

it("closes the unbind confirmation silently on session_binding_changed when the store shows the session unbound", async () => {
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable], [], {
    onClose,
    request: async () => {
      daemon.store.setState({ sessions: new Map([unbindOwner, { ...unbindable, bound_to: null }].map((s) => [s.id, s])) });
      throw new DaemonRequestError("raw daemon text", "session_binding_changed", { session: unbindable.id });
    },
  });
  await submitDialog();
  expect(onClose).toHaveBeenCalledOnce();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});

it.each([
  ["session_changing", { session: unbindable.id }],
  ["session_owner_changing", { session: unbindable.id, owner: unbindOwner.id }],
])("reports a %s refusal in the error alert and keeps the unbind confirmation open", async (code, params) => {
  const onClose = vi.fn();
  try {
    mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable], [], {
      onClose,
      request: async () => { throw new DaemonRequestError("raw daemon text", code, params); },
    });
    await submitDialog();
    expect(onClose).not.toHaveBeenCalled();
    expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
    const [alert] = errorAlerts.getSnapshot();
    expect(alert.error).toMatchObject({ code, params });
    expect(alert.title).toBe("Could not unbind the session");
    expect(alert.classification).toEqual({ kind: "actionable", recovery: code === "session_changing" ? "wait" : "ownerChanging" });
  } finally {
    while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge();
  }
});

it.each([
  ["released elsewhere", { ...unbindable, bound_to: null }],
  ["archived", { ...unbindable, status: "archived" as const }],
  ["deleted", undefined],
  ["left without its owner", "ownerGone" as const],
])("closes the unbind confirmation silently when the session is %s while it is open", async (_, now) => {
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "unbind-session", session: unbindable }, [unbindOwner, unbindable], [], { onClose });
  expect(onClose).not.toHaveBeenCalled();
  await act(async () => {
    const rest = now === "ownerGone" ? [unbindable] : [unbindOwner, ...(now ? [now] : [])];
    daemon.store.setState({ sessions: new Map(rest.map((s) => [s.id, s])) });
  });
  expect(onClose).toHaveBeenCalledOnce();
  expect(daemon.toastError).not.toHaveBeenCalled();
  expect(errorAlerts.getSnapshot()).toEqual([]);
});
