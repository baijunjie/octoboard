// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { enter, leave, mountForTooltip, openTooltip, withFakeTimers } from "../components/tooltipTestSupport";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { Account, Session } from "../protocol";
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
  callbacks: { onClose?: () => void; request?: Daemon["request"]; onSwitchAccount?: (sessionId: string, account: string | null) => Promise<void> } = {},
): { container: HTMLElement; daemon: Daemon } {
  const daemon: Daemon = {
    store: createStateStore({
      sessions: new Map(sessions.map((s) => [s.id, s])),
      settings: { auto_pull_repositories: false, default_clone_dir: "", accounts },
    }),
    request: vi.fn(callbacks.request) as never,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = mountForTooltip(
    <DaemonProvider value={daemon}>
      <RequestedDialog dialog={dialog} onClose={callbacks.onClose ?? (() => {})} onSessionOpened={() => {}} onSwitchAccount={callbacks.onSwitchAccount ?? (async () => {})} />
    </DaemonProvider>,
  );
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
  expect(hoverAgentIcon()).toBe(isolate("Default"));
});

it.each([
  ["a named account", { account_id: account.id }, isolate("Work")],
  ["an account that is no longer stored", { account_id: "gone", config_dir: "/home/dev/.claude-old" }, isolate("/home/dev/.claude-old")],
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

  expect(openTooltip()?.textContent ?? null).toBe(opensAtOnce ? isolate("Default") : null);
  settle();
  expect(openTooltip()?.textContent).toBe(isolate("Default"));

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

async function submitDialog(): Promise<void> {
  await act(async () => {
    document.body.querySelector("[role=alertdialog] form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
  });
}

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

it("switches only the selected session to the default account unless the checkbox is selected", async () => {
  const selected = { ...owner, account_id: account.id };
  const waiting = deferred();
  const onClose = vi.fn();
  const onSwitchAccount = vi.fn(() => waiting.promise);
  mount({ kind: "switch-account", session: selected, account: null, accountName: "Default" }, [selected, { ...selected, id: "s-other" }], [account], { onClose, onSwitchAccount });
  expect(document.body.querySelector<HTMLInputElement>('input[type="checkbox"]')!.checked).toBe(false);
  await submitDialog();
  expect(onSwitchAccount).toHaveBeenCalledExactlyOnceWith(selected.id, null);
  expect(document.body.querySelector('[role="progressbar"]')?.getAttribute("aria-valuemax")).toBe("1");
  expect(onClose).not.toHaveBeenCalled();
  await act(async () => waiting.resolve());
  expect(onClose).toHaveBeenCalledOnce();
});

it("switches a console's same-agent console, project and bound sessions sequentially with progress and a fixed scope", async () => {
  const hub = sessionOf("s-hub", console_.id, undefined, "Hub", "idle", { role: "console" });
  const bound = sessionOf("s-bound", console_.id, project.id, "Bound", "idle", { bound_to: hub.id });
  const excluded = [
    { ...owner, id: "already", account_id: account.id },
    { ...owner, id: "archived", status: "archived" as const },
    { ...owner, id: "other-agent", agent: "codex" as const },
    { ...owner, id: "other-console", console_id: "c-other" },
  ];
  const first = deferred();
  const second = deferred();
  const third = deferred();
  const onClose = vi.fn();
  const onSwitchAccount = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise).mockReturnValueOnce(third.promise);
  const { daemon } = mount({ kind: "switch-account", session: owner, account: account.id, accountName: account.name }, [hub, owner, bound, ...excluded], [account], { onClose, onSwitchAccount });
  checkAll();
  await submitDialog();
  expect(onSwitchAccount.mock.calls).toEqual([[hub.id, account.id]]);
  const progress = () => document.body.querySelector('[role="progressbar"]')!;
  expect(progress().getAttribute("aria-valuemax")).toBe("3");
  expect(progress().getAttribute("aria-valuenow")).toBe("0");
  expect(document.body.querySelector('input[type="checkbox"]')!.getAttribute("aria-readonly")).toBe("true");
  act(() => {
    const next = new Map(daemon.store.getState().sessions);
    next.set("late", { ...owner, id: "late" });
    daemon.store.setState({ sessions: next });
    document.body.querySelector<HTMLButtonElement>('button[aria-label="Close"]')!.click();
    document.body.querySelector('[role="alertdialog"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  });
  await submitDialog();
  expect(onClose).not.toHaveBeenCalled();
  expect(onSwitchAccount).toHaveBeenCalledTimes(1);
  await act(async () => first.resolve());
  expect(progress().getAttribute("aria-valuenow")).toBe("1");
  expect(onSwitchAccount.mock.calls).toEqual([[hub.id, account.id], [owner.id, account.id]]);
  await act(async () => second.resolve());
  expect(progress().getAttribute("aria-valuenow")).toBe("2");
  await act(async () => third.resolve());
  expect(onSwitchAccount.mock.calls).toEqual([[hub.id, account.id], [owner.id, account.id], [bound.id, account.id]]);
  expect(onClose).toHaveBeenCalledOnce();
});

it("continues after a failure and retries only failed sessions without relying on daemon events for successes", async () => {
  const failed = { ...owner, id: "failed", title: "Needs attention" };
  const last = { ...owner, id: "last" };
  const onClose = vi.fn();
  const onSwitchAccount = vi.fn().mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error("Copy failed")).mockResolvedValue(undefined);
  const { daemon } = mount({ kind: "switch-account", session: owner, account: account.id, accountName: account.name }, [owner, failed, last], [account], { onClose, onSwitchAccount });
  checkAll();
  await submitDialog();
  expect(onSwitchAccount.mock.calls).toEqual([[owner.id, account.id], [failed.id, account.id], [last.id, account.id]]);
  expect(document.body.querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("3");
  expect(document.body.querySelector('[role="alert"]')?.textContent).toContain("Needs attention");
  expect(document.body.querySelector('[role="alert"]')?.textContent).toContain("Copy failed");
  expect(onClose).not.toHaveBeenCalled();
  act(() => daemon.store.setState({ sessions: new Map([...daemon.store.getState().sessions, ["late", { ...owner, id: "late" }]]) }));
  await submitDialog();
  expect(onSwitchAccount.mock.calls).toEqual([[owner.id, account.id], [failed.id, account.id], [last.id, account.id], [failed.id, account.id]]);
  expect(onClose).toHaveBeenCalledOnce();
});

it.each(["archived", "on-target", "removed"] as const)("skips a snapshotted session that becomes %s before its turn", async (change) => {
  const next = { ...owner, id: "next" };
  const first = deferred();
  const onClose = vi.fn();
  const onSwitchAccount = vi.fn(() => first.promise);
  const { daemon } = mount({ kind: "switch-account", session: owner, account: account.id, accountName: account.name }, [owner, next], [account], { onClose, onSwitchAccount });
  checkAll();
  await submitDialog();
  act(() => {
    const sessions = new Map(daemon.store.getState().sessions);
    if (change === "removed") sessions.delete(next.id);
    else sessions.set(next.id, change === "archived" ? { ...next, status: "archived" } : { ...next, account_id: account.id });
    daemon.store.setState({ sessions });
  });
  await act(async () => first.resolve());
  expect(onSwitchAccount).toHaveBeenCalledExactlyOnceWith(owner.id, account.id);
  expect(onClose).toHaveBeenCalledOnce();
});


it("deletes an unused session without a confirmation only after a guarded file check", async () => {
  const unused = { ...owner, has_conversation: false };
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose,
    request: async (body) => body.type === "inspect_session_records"
      ? { type: "session_records_inspected", presence: "absent" }
      : { type: "ack" },
  });
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
  await act(async () => {});
  expect(daemon.request).toHaveBeenCalledWith({ type: "inspect_session_records", sessions: [unused.id] });
  expect(daemon.request).toHaveBeenCalledWith({ type: "delete_session", session: unused.id, only_if_unused_without_agent_records: true });
  expect(onClose).toHaveBeenCalledOnce();
  expect(document.querySelector("[role=alertdialog]")).toBeNull();
});

it.each(["present", "unknown"] as const)("confirms an unused session with %s agent records", async (presence) => {
  const unused = { ...owner, has_conversation: false };
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    request: async () => ({ type: "session_records_inspected", presence }),
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).not.toBeNull();
  expect(daemon.request).toHaveBeenCalledTimes(1);
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

it("falls back to confirmation if a direct deletion is refused after the file check", async () => {
  const unused = { ...owner, has_conversation: false };
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose,
    request: async (body) => {
      if (body.type === "delete_session") throw new Error("Session is now in use");
      return { type: "session_records_inspected", presence: "absent" };
    },
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).toBeNull();
  expect(daemon.request).toHaveBeenCalledTimes(3);
  expect(onClose).not.toHaveBeenCalled();
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
  const unused = { ...owner, has_conversation: false };
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    request: async () => { throw new Error("Connection lost"); },
  });
  await act(async () => {});
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(document.querySelector("[role=alertdialog] input[type=checkbox]")).not.toBeNull();
  expect(daemon.request).toHaveBeenCalledTimes(1);
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

it("does not act on an old unused session after the dialog subject changes", async () => {
  const unused = { ...owner, has_conversation: false };
  let resolve!: (value: { type: "session_records_inspected"; presence: "absent" }) => void;
  const waiting = new Promise<{ type: "session_records_inspected"; presence: "absent" }>((done) => { resolve = done; });
  const onClose = vi.fn();
  const { daemon } = mount({ kind: "delete-session", session: unused }, [unused], [], {
    onClose,
    request: () => waiting,
  });
  // A submitted prompt changes the same dialog’s current session before inspection completes.
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, owner]]) }));
  await act(async () => { resolve({ type: "session_records_inspected", presence: "absent" }); });
  expect(document.querySelector("[role=alertdialog]")).not.toBeNull();
  expect(daemon.request).toHaveBeenCalledTimes(2);
  expect(onClose).not.toHaveBeenCalled();
});
