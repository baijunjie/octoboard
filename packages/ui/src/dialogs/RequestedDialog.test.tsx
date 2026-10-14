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
function mount(dialog: DialogRequest, sessions: Session[], accounts: Account[] = []): { container: HTMLElement; daemon: Daemon } {
  const daemon: Daemon = {
    store: createStateStore({
      sessions: new Map(sessions.map((s) => [s.id, s])),
      settings: { auto_sync_repositories: false, default_clone_dir: "", accounts },
    }),
    request: vi.fn() as never,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = mountForTooltip(
    <DaemonProvider value={daemon}>
      <RequestedDialog dialog={dialog} onClose={() => {}} onSessionOpened={() => {}} onSwitchAccount={async () => {}} />
    </DaemonProvider>,
  );
  return { container, daemon };
}

/** The agent icon's tooltip trigger in the callout's first listed session: the status glyph's
 * neighbour, the second child of the row. */
function agentIcon(): HTMLElement {
  return document.body.querySelector<HTMLElement>("[role=alertdialog] li")!.children[1] as HTMLElement;
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
