// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { isolate } from "../i18n/isolate";
import type { Session } from "../protocol";
import { projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Daemon, State } from "../store";
import { parentConsole, project, renderDialog, hub1, buttons, optionElements, choose } from "./sessionDialogTestSupport";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

const UUID = "11111111-2222-3333-4444-555555555555";

const tab = (name: string) => Array.from(document.body.querySelectorAll<HTMLElement>('[role="tab"]')).find((element) => element.textContent === name);
const resumeTab = () => tab("Resume conversation");
const newTab = () => tab("New conversation");
const resumeField = () =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find((input) => input.labels?.[0]?.textContent === "Session ID");

/** Chooses to resume a conversation, if the dialog offers the choice. */
async function chooseResume(): Promise<void> {
  await act(async () => resumeTab()?.click());
}

it("leaves focus off the fields a tab switch mounts, so it stays on the tab", async () => {
  act(() => setInteractionModality("pointer"));
  const { container, root } = renderDialog([]);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => resumeTab()?.focus());
  await chooseResume();
  expect(document.activeElement).toBe(resumeTab());
  await act(async () => newTab()?.click());
  expect(document.activeElement).toBe(resumeTab());
});

/** Types `value` into the Session ID field. */
function typeResumeId(value: string): void {
  act(() => {
    const field = resumeField()!;
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(field, value);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

const record = (agent: string, account: string | null) => ({ type: "conversation_located", location: { kind: "record", agent, account } });
const heldBy = (kind: string, session?: string) => ({ type: "conversation_located", location: { kind, ...(session && { session }) } });

type Locate = object | ((id: string) => Promise<unknown>);

/** Answers `locate_conversation_to_resume` with `locate` (a reply, or a function of the ID that
 * gives one) and every other request with `other`, or an ack. */
function answerRequests(daemon: Daemon, locate: Locate, other?: (body: { type: string }) => Promise<unknown>): void {
  vi.mocked(daemon.request).mockImplementation((async (body: { type: string; agent_session_id?: string }) => {
    if (body.type !== "locate_conversation_to_resume") return other ? other(body) : { type: "ack" };
    return typeof locate === "function" ? locate(body.agent_session_id!) : locate;
  }) as never);
}

const settle = async () => act(async () => { await vi.advanceTimersByTimeAsync(300); });
const locateRequests = (daemon: Daemon) => vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === "locate_conversation_to_resume").map(([body]) => body);
const requestsOf = (daemon: Daemon, type: string) => vi.mocked(daemon.request).mock.calls.filter(([body]) => body.type === type).map(([body]) => body);
const openButton = () => buttons().find((button) => button.textContent === "Open")!;
const openDisabled = () => openButton().hasAttribute("disabled") || openButton().getAttribute("aria-disabled") === "true";
const claudeAccounts: Partial<State> = {
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [
    { id: "a-work", agent: "claude", name: "Work", config_dir: "/accounts/work" },
  ] },
};

/** Renders the dialog for a project (or, with `null`, for the console) over one console session,
 * with the lookup of the conversation answered by `locate` (an Agent record in Claude Code's default
 * account unless given). Timers are faked, as the lookup waits for the typing to rest. */
function renderForResume(projectOverride: Partial<typeof project> | null = {}, locate: Locate = record("claude", null), initial: Partial<State> = {}, sessions: Session[] = [hub1]) {
  vi.useFakeTimers();
  onTestFinished(() => { vi.useRealTimers(); });
  const rendered = renderDialog(sessions, initial, {}, { kind: "choose" }, projectOverride);
  onTestFinished(() => { act(() => rendered.root.unmount()); rendered.container.remove(); });
  answerRequests(rendered.daemon, locate);
  return rendered;
}

/** What the lookup the Resume tab makes found wrong with the ID, as the Alert under the field says it. */
const resumeAlert = () => document.body.querySelector<HTMLElement>('[data-slot="alert-root"]');

/** The error text of a field (its `FieldError`, an alert of its own). */
const fieldError = () => document.body.querySelector<HTMLElement>('[data-slot="field-error"] [role="alert"]');

/** The text of the line under the Session ID field. */
const resumeStatusText = () => document.body.querySelector('[role="status"]:not([data-slot="spinner"])')?.textContent ?? "";

const submit = async (label = "Open") => act(async () => buttons().find((button) => button.textContent === label)!.click());
const sentOpenSession = (daemon: Daemon) =>
  vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")?.[0] as unknown as Record<string, unknown> | undefined;

it("offers the choice of resuming only to a project session that is not fixed to an owner", () => {
  for (const [override, binding] of [[{}, choose], [{}, { kind: "unbound" } as const], [null, choose], [{}, { kind: "bound", to: hub1 } as const]] as const) {
    const { container, root } = renderDialog([hub1], {}, {}, binding, override);
    expect(resumeTab() !== undefined).toBe(override !== null && binding.kind !== "bound");
    act(() => root.unmount());
    container.remove();
  }
});

it("starts a new conversation by default, without the Session ID field", async () => {
  const { daemon } = renderForResume();
  expect(newTab()!.getAttribute("aria-selected")).toBe("true");
  expect(resumeField()).toBeUndefined();
  await submit();
  expect(sentOpenSession(daemon)).not.toHaveProperty("resume_agent_session_id");
});

it("sends the conversation to resume, trimmed, with no owner", async () => {
  const { daemon } = renderForResume();
  await chooseResume();
  expect(document.body.textContent).not.toContain("Report to");
  typeResumeId(`  ${UUID} `);
  await settle();
  await submit();
  expect(sentOpenSession(daemon)).toMatchObject({ resume_agent_session_id: UUID, bound_to: undefined });
});

it.each([
  { name: "empty", id: "", message: "Enter the session ID of the conversation to resume." },
  { name: "not a UUID", id: "my-title", message: "Enter a session ID like" },
])("holds back a $name conversation ID and says so under the field", async ({ id, message }) => {
  const { daemon } = renderForResume();
  await chooseResume();
  if (id) typeResumeId(id);
  expect(document.body.textContent).not.toContain(message);
  await submit();
  expect(document.body.textContent).toContain(message);
  expect(document.activeElement).toBe(resumeField());
  expect(sentOpenSession(daemon)).toBeUndefined();
  // A literal in the message takes the error's tone.
  for (const code of document.body.querySelectorAll('[role="alert"] code')) expect(code.getAttribute("data-color")).toBe("danger");
});

it("reports the daemon's refusal of the conversation in the error alert dialog and keeps the field as it was", async () => {
  const { daemon } = renderForResume();
  answerRequests(daemon, record("claude", null), () => Promise.reject(
    new DaemonRequestError("plain", "conversation_to_resume_not_found", { agent: "Claude Code", path: "/Users/dev/.claude", id: UUID }),
  ));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit();
  const field = resumeField()!;
  // It comes after the submit, so it is the error alert dialog's, not a mark on the field.
  expect(errorAlerts.getSnapshot().some((alert) => alert.error instanceof DaemonRequestError && alert.error.code === "conversation_to_resume_not_found")).toBe(true);
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  expect(field.getAttribute("aria-invalid")).not.toBe("true");
  expect(resumeAlert()).toBeNull();
  expect(field.value).toBe(UUID);
  // Where focus returns once the alert is acknowledged.
  expect(document.activeElement).toBe(field);
});

const nameInput = () => Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find((input) => input.labels?.[0]?.textContent === "Name");
const typeName = (value: string) => act(() => {
  const name = nameInput()!;
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(name, value);
  name.dispatchEvent(new Event("input", { bubbles: true }));
});
const hasAdvanced = () => buttons().some((button) => button.textContent === "Advanced");
/** Whether the Agent and Account pickers are shown, and all of them disabled. */
const agentTriggers = () => Array.from(document.body.querySelectorAll<HTMLElement>('[data-slot="select-trigger"]'));
const allDisabled = (elements: HTMLElement[]) => elements.length > 0 && elements.every((element) => element.hasAttribute("disabled") || element.getAttribute("aria-disabled") === "true" || element.hasAttribute("data-disabled"));
const spinner = () => document.body.querySelector('[data-slot="spinner"]');

it("holds only the Session ID until the conversation is found, with a decorative spinner while it is looked up", async () => {
  const { daemon } = renderForResume({ default_agent: "codex" }, record("claude", "a-work"), claudeAccounts);
  await chooseResume();
  expect(resumeStatusText()).toBe("");
  expect(nameInput()).toBeUndefined();
  expect(agentTriggers()).toHaveLength(0);
  expect(hasAdvanced()).toBe(false);
  expect(spinner()).toBeNull();
  typeResumeId("11111111-2222-3333-4444");
  await settle();
  expect(locateRequests(daemon)).toEqual([]);
  typeResumeId(UUID);
  expect(resumeStatusText()).toBe("Looking up the conversation…");
  expect(spinner()).not.toBeNull();
  expect(spinner()!.getAttribute("aria-hidden")).toBe("true");
  expect(resumeField()!.parentElement!.contains(spinner())).toBe(true);
  expect(nameInput()).toBeUndefined();
  expect(openDisabled()).toBe(true);
  await settle();
  expect(locateRequests(daemon)).toEqual([{ type: "locate_conversation_to_resume", project: "p-1", agent_session_id: UUID }]);
  expect(spinner()).toBeNull();
});

it("opens a new session on the agent and account of a record, which are shown disabled", async () => {
  const { daemon } = renderForResume({ default_agent: "codex" }, record("claude", "a-work"), claudeAccounts);
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeStatusText()).toBe(`Found a record on Claude Code · ${isolate("Work")}`);
  // The launch settings below have pickers of their own, which stay enabled.
  expect(allDisabled(agentTriggers().slice(0, 2))).toBe(true);
  expect(agentTriggers().slice(0, 2).map((trigger) => trigger.textContent)).toEqual(["Claude Code", "Work"]);
  expect(nameInput()!.disabled).toBe(false);
  expect(hasAdvanced()).toBe(true);
  expect(openDisabled()).toBe(false);
  await submit();
  expect(sentOpenSession(daemon)).toMatchObject({ agent: "claude", account: "a-work", resume_agent_session_id: UUID });
});

it("refuses a resumed record on an agent whose accounts are all turned off instead of using another agent", async () => {
  const { daemon } = renderForResume({ default_agent: "codex" }, record("claude", "a-work"), {
    settings: {
      ...claudeAccounts.settings!,
      disabled_default_accounts: ["claude"],
      accounts: claudeAccounts.settings!.accounts.map((account) => ({ ...account, disabled: true })),
    },
  });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(agentTriggers().slice(0, 2).map((trigger) => trigger.textContent)).toEqual(["Claude Code", "Work"]);
  expect(document.body.textContent).toContain("Account Work is disabled");
  expect(openDisabled()).toBe(true);
  expect(requestsOf(daemon, "open_session")).toEqual([]);
});

it("keeps a resumed record's account when automatic selection would pick another", async () => {
  const now = Date.now();
  const weekly = (used_percent: number, resets_at: number) => ({ kind: "weekly" as const, label: null, used_percent, resets_at, window_minutes: 7 * 24 * 60 });
  const { daemon } = renderForResume({ default_agent: "codex" }, record("claude", "a-work"), {
    settings: {
      ...claudeAccounts.settings!,
      auto_select_account: true,
      accounts: [
        ...claudeAccounts.settings!.accounts,
        { id: "a-soon", agent: "claude", name: "Soon", config_dir: "/accounts/soon" },
      ],
    },
    accountQuotas: [
      { agent: "claude", account: "a-work", fetched_at: now, plan: null, unavailable: null, windows: [weekly(10, now + 7 * 24 * 60 * 60 * 1000)] },
      { agent: "claude", account: "a-soon", fetched_at: now, plan: null, unavailable: null, windows: [weekly(1, now + 3_600_000)] },
    ],
  });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit();
  expect(sentOpenSession(daemon)).toMatchObject({ agent: "claude", account: "a-work", resume_agent_session_id: UUID });
});

const archived = sessionOf("s-old", parentConsole.id, project.id, "Old work", "archived", { account_id: "a-work" });
const running = sessionOf("s-run", parentConsole.id, project.id, "Busy", "working");
const interrupted = sessionOf("s-int", parentConsole.id, project.id, "Paused", "interrupted", { account_id: "a-work" });

it("announces exhausted quota only when the selected conversation requires a new launch", async () => {
  const { daemon, onResume, onOpened } = renderForResume(
    { claude_account_id: { kind: "account", id: "a-work" } },
    heldBy("live_session", interrupted.id),
    { ...claudeAccounts, sessions: new Map([[interrupted.id, interrupted]]) },
  );
  const quotaAlert = fieldError;
  expect(quotaAlert()).toBeNull();
  expect(openDisabled()).toBe(false);
  // The quota arrives after the form opened, so it is announced as the account field's error.
  act(() => daemon.store.setState({ accountQuotas: [null, "a-work"].map((account) => ({
    agent: "claude" as const, account, fetched_at: Date.now(), plan: null, unavailable: null,
    windows: [{ kind: "session" as const, label: null, used_percent: 100, resets_at: Date.now() + 3_600_000, window_minutes: 300 }],
  })) }));
  const assertQuotaAlert = () => {
    const alert = quotaAlert();
    expect(alert?.textContent).toContain("Work cannot be used for now because it has reached its session limit.");
    expect(alert?.querySelector("div,p,ul,pre,h1,h2,h3")).toBeNull();
    expect(openDisabled()).toBe(true);
  };
  assertQuotaAlert();
  await submit();
  expect(sentOpenSession(daemon)).toBeUndefined();

  await chooseResume();
  expect(quotaAlert()).toBeNull();
  typeResumeId(UUID);
  expect(quotaAlert()).toBeNull();
  expect(openDisabled()).toBe(true);
  await settle();
  expect(quotaAlert()).toBeNull();
  expect(resumeStatusText()).toContain("Found interrupted session");
  expect(buttons().find((button) => button.textContent === "Resume")?.hasAttribute("disabled")).toBe(false);
  await submit("Resume");
  expect(onResume).toHaveBeenCalledExactlyOnceWith(interrupted.id);
  expect(onOpened).toHaveBeenCalledExactlyOnceWith(interrupted.id);
  expect(sentOpenSession(daemon)).toBeUndefined();

  answerRequests(daemon, record("claude", "a-work"));
  typeResumeId("11111111-2222-3333-4444-555555555556");
  expect(quotaAlert()).toBeNull();
  await settle();
  expect(resumeStatusText()).toBe(`Found a record on Claude Code · ${isolate("Work")}`);
  assertQuotaAlert();
  await submit();
  expect(sentOpenSession(daemon)).toBeUndefined();
  expect(onResume).toHaveBeenCalledTimes(1);
  expect(onOpened).toHaveBeenCalledTimes(1);
});

it("holds Open back, with why, when the only account left is changing its sharing", () => {
  const { daemon } = renderForResume({ claude_account_id: { kind: "account", id: "a-work" } }, record("claude", null), {
    ...claudeAccounts,
    accountQuotas: [{
      agent: "claude", account: null, fetched_at: Date.now(), plan: null, unavailable: null,
      windows: [{ kind: "session", label: null, used_percent: 100, resets_at: Date.now() + 3_600_000, window_minutes: 300 }],
    }],
    accountSharing: new Map([["a-work", { account: "a-work", progress: { share: true, phase: "changing" as const, done: 0, total: 2 } }]]),
  });
  expect(fieldError()?.textContent).toContain("Choose another account or wait for the change to finish.");
  expect(openDisabled()).toBe(true);
  expect(daemon.request).not.toHaveBeenCalled();
});

it("keeps focus in the Session ID field while the lookup comes and goes", async () => {
  act(() => setInteractionModality("pointer"));
  renderForResume({}, record("claude", null));
  await chooseResume();
  const field = resumeField()!;
  act(() => field.focus());
  typeResumeId(UUID);
  expect(spinner()).not.toBeNull();
  expect(document.activeElement).toBe(field);
  await settle();
  expect(spinner()).toBeNull();
  expect(resumeField()).toBe(field);
  expect(document.activeElement).toBe(field);
});

it("prefills an archived session's own name, not its terminal title, and shows it as the placeholder when cleared", async () => {
  const titled = sessionOf("s-old", parentConsole.id, project.id, "Old work", "archived", { terminal_title: "Fixing tests" });
  const { daemon } = renderForResume({}, heldBy("archived_session", titled.id), { sessions: new Map([[titled.id, titled]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeStatusText()).toContain("Fixing tests");
  expect(nameInput()!.value).toBe("Old work");
  expect(nameInput()!.placeholder).toBe("Old work");
  typeName("");
  expect(nameInput()!.placeholder).toBe("Old work");
  await submit("Reopen");
  expect(requestsOf(daemon, "rename_session")).toEqual([]);
});

it("shows the session's own name once the archived session is no longer archived, whatever was typed", async () => {
  const { daemon } = renderForResume({}, heldBy("archived_session", archived.id), { sessions: new Map([[archived.id, archived]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  typeName("Renamed");
  act(() => daemon.store.setState((state) => ({ ...state, sessions: new Map([[archived.id, { ...archived, status: "interrupted" as const }]]) })));
  expect(nameInput()!.value).toBe("Old work");
  expect(nameInput()!.disabled).toBe(true);
});

it.each(["conversation_in_session", "conversation_outside_project"])("asks the lookup again after the daemon refuses a record with %s, and goes by the new answer", async (code) => {
  let asked = 0;
  const { daemon } = renderForResume({}, record("claude", null), { sessions: new Map([[archived.id, archived]]) });
  answerRequests(daemon, async () => (asked++ === 0 ? record("claude", null) : heldBy("archived_session", archived.id)), (body) =>
    body.type === "open_session" ? Promise.reject(new DaemonRequestError("plain", code, {})) : Promise.resolve({ type: "ack" }));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit();
  expect(locateRequests(daemon)).toHaveLength(1);
  await settle();
  expect(locateRequests(daemon)).toHaveLength(2);
  expect(resumeStatusText()).toContain("Found archived session");
  expect(buttons().some((button) => button.textContent === "Reopen")).toBe(true);
  expect(resumeField()!.getAttribute("aria-invalid")).not.toBe("true");
});

it("moves focus to the Session ID field when the focused Name field turns disabled", async () => {
  act(() => setInteractionModality("pointer"));
  const { daemon } = renderForResume({}, heldBy("archived_session", archived.id), { sessions: new Map([[archived.id, archived]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  act(() => nameInput()!.focus());
  expect(document.activeElement).toBe(nameInput());
  act(() => daemon.store.setState((state) => ({ ...state, sessions: new Map([[archived.id, { ...archived, status: "interrupted" as const }]]) })));
  expect(document.activeElement).toBe(resumeField());
});

it("moves focus to the Session ID field when the focused Name field is disabled and blurs before the state catches up", async () => {
  act(() => setInteractionModality("pointer"));
  const { daemon } = renderForResume({}, heldBy("archived_session", archived.id), { sessions: new Map([[archived.id, archived]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  const name = nameInput()!;
  act(() => name.focus());
  // A browser blurs a control as it is disabled; jsdom does not, so it is done by hand.
  act(() => {
    name.disabled = true;
    name.blur();
  });
  act(() => daemon.store.setState((state) => ({ ...state, sessions: new Map([[archived.id, { ...archived, status: "interrupted" as const }]]) })));
  expect(document.activeElement).toBe(resumeField());
});

it("moves focus to the Session ID field when the focused record fields go", async () => {
  act(() => setInteractionModality("pointer"));
  const { daemon } = renderForResume();
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  act(() => nameInput()!.focus());
  answerRequests(daemon, heldBy("not_found"));
  typeResumeId("11111111-2222-3333-4444-555555555556");
  await settle();
  expect(nameInput()).toBeUndefined();
  expect(document.activeElement).toBe(resumeField());
});

it("keeps the answer for a record when the daemon says the conversation is not there", async () => {
  const { daemon } = renderForResume();
  answerRequests(daemon, record("claude", null), () => Promise.reject(new DaemonRequestError("plain", "conversation_to_resume_not_found", { agent: "Claude Code", path: "/p", id: UUID })));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit();
  await settle();
  expect(locateRequests(daemon)).toHaveLength(1);
  expect(openDisabled()).toBe(false);
});

it("reopens the archived session that holds the conversation, its name prefilled and left alone unless changed", async () => {
  const { daemon, onOpened, onClose, onResume } = renderForResume({}, heldBy("archived_session", archived.id), { ...claudeAccounts, sessions: new Map([[archived.id, archived]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeStatusText()).toContain("Found archived session");
  expect(resumeStatusText()).toContain("Old work");
  expect(nameInput()!.value).toBe("Old work");
  expect(nameInput()!.disabled).toBe(false);
  expect(allDisabled(agentTriggers())).toBe(true);
  expect(agentTriggers().map((trigger) => trigger.textContent)).toEqual(["Claude Code", "Work"]);
  expect(hasAdvanced()).toBe(false);
  expect(buttons().some((button) => button.textContent === "Open")).toBe(false);
  await submit("Reopen");
  expect(onResume).toHaveBeenCalledWith("s-old");
  expect(requestsOf(daemon, "rename_session")).toEqual([]);
  expect(requestsOf(daemon, "open_session")).toEqual([]);
  expect(onOpened).toHaveBeenCalledWith("s-old");
  expect(onClose).toHaveBeenCalled();
});

it("shows only the agent for an archived session whose account is unknown", async () => {
  const unknown = sessionOf("s-old", parentConsole.id, project.id, "Old work", "archived", { account_id: "a-gone" });
  renderForResume({}, heldBy("archived_session", unknown.id), { sessions: new Map([[unknown.id, unknown]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(agentTriggers().map((trigger) => trigger.textContent)).toEqual(["Claude Code"]);
});

it("renames the reopened session when the name is changed", async () => {
  const { daemon, onResume } = renderForResume({}, heldBy("archived_session", archived.id), { sessions: new Map([[archived.id, archived]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  typeName("Renamed");
  await submit("Reopen");
  expect(onResume).toHaveBeenCalledTimes(1);
  expect(requestsOf(daemon, "rename_session")).toEqual([{ type: "rename_session", session: "s-old", title: "Renamed" }]);
});

it("does not rename the reopened session to an empty name", async () => {
  const { daemon } = renderForResume({}, heldBy("archived_session", archived.id), { sessions: new Map([[archived.id, archived]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  typeName("  ");
  await submit("Reopen");
  expect(requestsOf(daemon, "rename_session")).toEqual([]);
});

it("toasts a rename that fails after the reopen, and still selects the session and closes", async () => {
  const { daemon, onOpened, onClose, onResume } = renderForResume({}, heldBy("archived_session", archived.id), { sessions: new Map([[archived.id, archived]]) });
  answerRequests(daemon, heldBy("archived_session", archived.id), (body) =>
    body.type === "rename_session" ? Promise.reject(new DaemonRequestError("plain", "unknown_session", { session: "s-old" })) : Promise.resolve({ type: "ack" }));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  typeName("Renamed");
  await submit("Reopen");
  expect(onResume).toHaveBeenCalledWith("s-old");
  expect(daemon.toastError).toHaveBeenCalledTimes(1);
  expect(onOpened).toHaveBeenCalledWith("s-old");
  expect(onClose).toHaveBeenCalled();
});

it("resumes an interrupted session through the resume path, with name, agent and account disabled", async () => {
  const { daemon, onOpened, onClose, onResume } = renderForResume({}, heldBy("live_session", interrupted.id), { ...claudeAccounts, sessions: new Map([[interrupted.id, interrupted]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeStatusText()).toContain("Found interrupted session");
  expect(nameInput()!.value).toBe("Paused");
  expect(nameInput()!.disabled).toBe(true);
  expect(allDisabled(agentTriggers())).toBe(true);
  expect(hasAdvanced()).toBe(false);
  await submit("Resume");
  expect(onResume).toHaveBeenCalledWith("s-int");
  expect(requestsOf(daemon, "rename_session")).toEqual([]);
  expect(onOpened).toHaveBeenCalledWith("s-int");
  expect(onClose).toHaveBeenCalled();
});

it("shows the daemon's reason in the dialog when resuming an interrupted session is rejected", async () => {
  const { onOpened, onClose, onResume } = renderForResume({}, heldBy("live_session", interrupted.id), { sessions: new Map([[interrupted.id, interrupted]]) });
  onResume.mockRejectedValueOnce(new DaemonRequestError("plain", "unknown_session", { session: "s-int" }));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit("Resume");
  expect(errorAlerts.getSnapshot().some((alert) => alert.error instanceof DaemonRequestError && alert.error.code === "unknown_session")).toBe(true);
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  expect(onOpened).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
});

it("goes to a running session from the footer, with its fields disabled", async () => {
  const { onOpened, onClose, onResume } = renderForResume({}, heldBy("live_session", running.id), { sessions: new Map([[running.id, running]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeStatusText()).toContain("Found running session");
  expect(nameInput()!.value).toBe("Busy");
  expect(nameInput()!.disabled).toBe(true);
  expect(allDisabled(agentTriggers())).toBe(true);
  expect(hasAdvanced()).toBe(false);
  expect(buttons().some((button) => button.textContent === "Open")).toBe(false);
  await submit("Go to session");
  expect(onResume).not.toHaveBeenCalled();
  expect(onOpened).toHaveBeenCalledWith("s-run");
  expect(onClose).toHaveBeenCalled();
});

it("takes a named session by its state now, not by the state the lookup saw", async () => {
  const { onResume } = renderForResume({}, heldBy("archived_session", running.id), { sessions: new Map([[running.id, running]]) });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(buttons().some((button) => button.textContent === "Go to session")).toBe(true);
  expect(buttons().some((button) => button.textContent === "Reopen")).toBe(false);
  expect(onResume).not.toHaveBeenCalled();
});

it("asks again when the session the lookup named is not in the store, and goes by the new answer", async () => {
  let asked = 0;
  const { daemon } = renderForResume({}, async () => (asked++ === 0 ? heldBy("archived_session", "s-unknown") : heldBy("not_found")));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeStatusText()).toBe("Looking up the conversation…");
  expect(openDisabled()).toBe(true);
  await settle();
  expect(locateRequests(daemon)).toHaveLength(2);
  expect(resumeStatusText()).toBe("");
  expect(fieldError()?.textContent).toContain("No conversation with this ID was found");
  expect(openDisabled()).toBe(true);
});

it("says the agent that holds the conversation is no longer offered, and never opens the pickers' fallback", async () => {
  const { daemon } = renderForResume({}, record("codex", null), { settings: { ...claudeAccounts.settings!, added_agents: ["claude"] } });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeAlert()?.textContent).toContain("no longer available");
  expect(resumeStatusText()).toBe("");
  expect(buttons().some((button) => button.textContent === "Advanced")).toBe(false);
  expect(openDisabled()).toBe(true);
  await submit();
  expect(sentOpenSession(daemon)).toBeUndefined();
});

it("keeps the dialog open with the reason when the reopen is refused, without renaming or selecting", async () => {
  const { daemon, onOpened, onClose, onResume } = renderForResume({}, heldBy("archived_session", archived.id), { sessions: new Map([[archived.id, archived]]) });
  onResume.mockRejectedValue(new DaemonRequestError("plain", "conversation_unresumable", { agent: "Claude Code", session: "s-old" }));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit("Reopen");
  // As any refused action of a dialog: the reason goes to the acknowledgement queue.
  expect(errorAlerts.getSnapshot().some((alert) => alert.error instanceof DaemonRequestError && alert.error.code === "conversation_unresumable")).toBe(true);
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  expect(requestsOf(daemon, "rename_session")).toEqual([]);
  expect(onOpened).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
});

it.each([
  { name: "belongs to a session outside the project", reply: heldBy("outside_project", "s-unknown"), message: "already belongs to a session outside this project", found: resumeAlert },
  // No conversation having the ID is a verdict on the ID, so it is the field's own error.
  { name: "is in no account", reply: heldBy("not_found"), message: "No conversation with this ID was found in any account.", found: fieldError },
])("says under the field that the conversation $name, and holds Open back", async ({ reply, message, found }) => {
  const { daemon } = renderForResume({}, reply);
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(found()?.textContent).toContain(message);
  expect(openDisabled()).toBe(true);
  await submit();
  expect(sentOpenSession(daemon)).toBeUndefined();
});

const otherProject = projectOf("p-2", parentConsole.id, "Billing");
const elsewhere = sessionOf("s-else", parentConsole.id, otherProject.id, "Invoices", "idle");
const consoleHeld = sessionOf("s-cons", parentConsole.id, undefined, "Hub work", "idle");

it.each([
  { name: "a session of another project", holder: elsewhere, message: "This conversation is already held by the session Invoices in Billing.", names: ["Invoices", "Billing"] },
  { name: "a console session", holder: consoleHeld, message: "This conversation is already held by the console session Hub work in Main.", names: ["Hub work", "Main"] },
])("names $name that holds the conversation, and where", async ({ holder, message, names }) => {
  const initial = { sessions: new Map([[holder.id, holder]]), projects: new Map([[otherProject.id, otherProject]]), consoles: new Map([[parentConsole.id, parentConsole]]) };
  renderForResume({}, heldBy("outside_project", holder.id), initial);
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  const reason = resumeAlert()!;
  expect(reason.textContent).toContain(message);
  expect(Array.from(reason.querySelectorAll("[data-inline-reference]"), (chip) => [chip.textContent, chip.getAttribute("dir")])).toEqual(names.map((name) => [name, "auto"]));
  expect(reason.textContent).not.toMatch(/[\u2066-\u2069]/);
  expect(openDisabled()).toBe(true);
  expect(nameInput()).toBeUndefined();
});

it("reports the open-time refusal of a conversation held elsewhere in the error alert dialog", async () => {
  const { daemon } = renderForResume({}, record("claude", null), { sessions: new Map([[elsewhere.id, elsewhere]]), projects: new Map([[otherProject.id, otherProject]]) });
  answerRequests(daemon, record("claude", null), (body) =>
    body.type === "open_session" ? Promise.reject(new DaemonRequestError("plain", "conversation_outside_project", { id: UUID, session: elsewhere.id })) : Promise.resolve({ type: "ack" }));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit();
  expect(errorAlerts.getSnapshot().some((alert) => alert.error instanceof DaemonRequestError && alert.error.code === "conversation_outside_project")).toBe(true);
  onTestFinished(() => { while (errorAlerts.getSnapshot().length) errorAlerts.acknowledge(); });
  expect(resumeAlert()).toBeNull();
});

it("shows a failed lookup under the field with the daemon's reason, and holds Open back", async () => {
  const { daemon } = renderForResume({}, () => Promise.reject(new DaemonRequestError("plain", "unknown_project", { id: "p-1" })));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeAlert()).not.toBeNull();
  // The Alert describes the field it is under.
  expect(resumeField()!.getAttribute("aria-describedby")?.split(" ")).toContain(resumeAlert()!.parentElement!.id);
  expect(openDisabled()).toBe(true);
  await submit();
  expect(sentOpenSession(daemon)).toBeUndefined();
});

it("drops the answer for an ID since changed, and asks again for the new one", async () => {
  const replies = new Map<string, (event: unknown) => void>();
  const { daemon } = renderForResume({}, (id) => new Promise((resolve) => replies.set(id, resolve)));
  await chooseResume();
  const other = "11111111-2222-3333-4444-555555555556";
  typeResumeId(UUID);
  await settle();
  typeResumeId(other);
  await settle();
  expect(locateRequests(daemon)).toHaveLength(2);
  await act(async () => replies.get(UUID)!(heldBy("not_found")));
  expect(resumeAlert()).toBeNull();
  expect(resumeStatusText()).toBe("Looking up the conversation…");
  await act(async () => replies.get(other)!(record("claude", null)));
  expect(resumeStatusText()).toBe(`Found a record on Claude Code · ${isolate("Default")}`);
  await submit();
  expect(sentOpenSession(daemon)).toMatchObject({ agent: "claude", account: null, resume_agent_session_id: other });
});

it("brings back the owner choice and sends no conversation after choosing a new one again", async () => {
  const { daemon } = renderForResume();
  await act(async () => buttons().find((b) => b.textContent?.includes("No console session"))!.click());
  await act(async () => optionElements().find((o) => o.textContent === "Hub")!.click());
  await chooseResume();
  typeResumeId(UUID);
  await act(async () => newTab()!.click());
  expect(document.body.textContent).toContain("Report to");
  expect(resumeField()).toBeUndefined();
  await submit();
  expect(sentOpenSession(daemon)).toMatchObject({ bound_to: "s-hub-1" });
  expect(sentOpenSession(daemon)).not.toHaveProperty("resume_agent_session_id");
});
