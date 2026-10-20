// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { errorAlerts } from "../errorAlerts";
import { isolate } from "../i18n/isolate";
import type { Agent, AgentAvailability, Session } from "../protocol";
import type { NewSessionBinding } from "./dialogRequest";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { trackMountRefocus } from "./dialogTestSupport";
import { SessionDialog } from "./SessionDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const parentConsole = consoleOf("c-1", "Main");
const project = projectOf("p-1", parentConsole.id, "Search API");

function fakeDaemon(initial: Partial<State> = {}): Daemon {
  return {
    store: createStateStore(initial),
    request: vi.fn(async () => ({ type: "ack" })) as Daemon["request"],
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

/** An `agentAvailability` map with every agent `unavailable` — "no agent available at all". Named
 * distinctly from `agents.ts`'s own `noAgentState`, which this is only a fixture for. */
function everyAgentUnavailable(): Map<Agent, AgentAvailability> {
  const agents: Agent[] = ["claude", "codex", "grok"];
  return new Map(agents.map((agent) => [agent, { agent, availability: "unavailable" as const }]));
}

/** Renders the dialog and returns its container, root and daemon, so a test can query the rendered
 * body, read the requests sent and unmount it, along with `pastMountRefocus`, which the test awaits
 * before a step whose focus it asserts (see `trackMountRefocus`). */
function renderDialog(
  sessions: Session[],
  initial: Partial<State> = {},
  consoleOverride: Partial<typeof parentConsole> = {},
  binding: NewSessionBinding = { kind: "choose" },
  projectOverride: Partial<typeof project> | null = {},
): { container: HTMLElement; root: ReturnType<typeof createRoot>; daemon: Daemon; onClose: () => void; onOpened: (sessionId: string) => void; onResume: ReturnType<typeof vi.fn>; pastMountRefocus: () => Promise<void> } {
  const daemon = fakeDaemon(initial);
  const onClose = vi.fn();
  const onOpened = vi.fn();
  const onResume = vi.fn(async () => {});
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <SessionDialog
          console={{ ...parentConsole, ...consoleOverride }}
          project={projectOverride === null ? undefined : { ...project, ...projectOverride }}
          binding={binding}
          sessions={sessions}
          onClose={onClose}
          onOpened={onOpened}
          onResume={onResume}
        />
      </DaemonProvider>,
    ),
  );
  return { container, root, daemon, onClose, onOpened, onResume, pastMountRefocus: trackMountRefocus() };
}

const hub1 = sessionOf("s-hub-1", parentConsole.id, undefined, "Hub", "idle", { colour: "teal", started_at: 100 });
const hub2 = sessionOf("s-hub-2", parentConsole.id, undefined, "Hub 2", "working", { colour: "pink", started_at: 200 });
const hubArchived = sessionOf("s-hub-3", parentConsole.id, undefined, "Hub 3", "archived", { colour: "cyan" });

const buttons = () => Array.from(document.body.querySelectorAll("button"));
const optionElements = () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]'));

/** Renders the dialog over `sessions`, picks the owner option named `choose` if given, submits, and
 * returns the `open_session` request sent, the owner select's option labels (empty when the dialog
 * offers no choice) and the dialog's text. */
async function openWith(
  sessions: Session[],
  choose?: string,
  binding?: NewSessionBinding,
): Promise<{ request: Record<string, unknown>; options: string[]; text: string }> {
  const { container, root, daemon } = renderDialog(sessions, {}, {}, binding);
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  // The owner select is the one trigger showing its default, "none".
  const ownerTrigger = buttons().find((b) => b.textContent?.includes("No console session"));
  let options: string[] = [];
  if (ownerTrigger) {
    await act(async () => ownerTrigger.click());
    options = optionElements().map((o) => o.textContent ?? "");
    if (choose) await act(async () => optionElements().find((o) => o.textContent === choose)?.click());
  }
  const text = document.body.textContent ?? "";
  await act(async () => buttons().find((b) => b.textContent === "Open")?.click());
  const request = vi.mocked(daemon.request).mock.calls[0][0] as unknown as Record<string, unknown>;
  act(() => root.unmount());
  container.remove();
  return { request, options, text };
}

it("opens with focus in the Name field, so Enter submits at once", () => {
  act(() => setInteractionModality("pointer"));
  const { container, root } = renderDialog([]);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  expect((document.activeElement as HTMLInputElement).labels?.[0]?.textContent).toBe("Name");
});

const choose: NewSessionBinding = { kind: "choose" };
it.each([
  { name: "defaults to none and offers every live console session", sessions: [hub1, hub2, hubArchived], pick: undefined, binding: choose, options: ["No console session", "Hub 2", "Hub"], boundTo: undefined },
  { name: "binds to the console session chosen", sessions: [hub1, hub2, hubArchived], pick: "Hub", binding: choose, options: ["No console session", "Hub 2", "Hub"], boundTo: "s-hub-1" },
  { name: "offers no choice with no live console session", sessions: [hubArchived], pick: undefined, binding: choose, options: [], boundTo: undefined },
  { name: "offers no choice and sends no binding when unbound", sessions: [hub1, hub2], pick: undefined, binding: { kind: "unbound" } as const, options: [], boundTo: undefined },
  { name: "binds to the fixed owner without offering a choice", sessions: [hub1, hub2], pick: undefined, binding: { kind: "bound", to: hub1 } as const, options: [], boundTo: "s-hub-1", fixedLine: `Reports to ${isolate("Hub")}` },
])("$name", async ({ sessions, pick, binding, options, boundTo, fixedLine }) => {
  const result = await openWith(sessions, pick, binding);
  expect(result.options).toEqual(options);
  expect(result.request.bound_to).toBe(boundTo);
  expect(result.text.includes("Reports to")).toBe(fixedLine !== undefined);
  if (fixedLine) expect(result.text).toContain(fixedLine);
});

it("cannot open a session for a fixed owner that is no longer live", () => {
  const { container, root } = renderDialog([{ ...hub1, status: "archived" }], {}, {}, { kind: "bound", to: hub1 });
  expect(buttons().find((b) => b.textContent === "Open")?.hasAttribute("disabled")).toBe(true);
  expect(document.body.textContent).toContain(`${isolate("Hub")} is no longer available`);
  act(() => root.unmount());
  container.remove();
});

it("keeps the owner select, back on none, when the chosen console session is archived under it", async () => {
  const { container, root, daemon } = renderDialog([hub1]);
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  await act(async () => buttons().find((b) => b.textContent?.includes("No console session"))?.click());
  await act(async () => optionElements().find((o) => o.textContent === "Hub")?.click());
  expect(buttons().some((b) => b.textContent === "Hub")).toBe(true);

  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <SessionDialog
          console={parentConsole}
          project={project}
          binding={choose}
          sessions={[{ ...hub1, status: "archived" }]}
          onClose={() => {}}
          onResume={async () => {}}
          onOpened={() => {}}
        />
      </DaemonProvider>,
    ),
  );
  const ownerTrigger = buttons().find((b) => b.textContent?.includes("No console session"));
  expect(ownerTrigger).toBeDefined();
  await act(async () => ownerTrigger?.click());
  expect(optionElements().map((o) => o.textContent)).toEqual(["No console session"]);

  await act(async () => buttons().find((b) => b.textContent === "Open")?.click());
  expect(vi.mocked(daemon.request).mock.calls[0][0]).toMatchObject({ type: "open_session", bound_to: undefined });
  act(() => root.unmount());
  container.remove();
});

it("closes and reports a plain alert, with no action, when no agent is available at all", () => {
  const { container, root, onClose, daemon } = renderDialog([], { agentAvailability: everyAgentUnavailable() });
  expect(onClose).toHaveBeenCalled();
  expect(daemon.toastError).toHaveBeenCalledWith(expect.objectContaining({ message: "No agent is available to open a session.", recovery: "none" }));
  act(() => root.unmount());
  container.remove();
});

it("opens on a selectable agent, with the console's account for it, and sends both", async () => {
  // Claude Code, the console's default agent, is known to be missing; Codex is the first left.
  const { container, root, daemon } = renderDialog(
    [],
    {
      agentAvailability: new Map<Agent, AgentAvailability>([
        ["claude", { agent: "claude", availability: "unavailable" }],
        ["codex", { agent: "codex", availability: "available" }],
        ["grok", { agent: "grok", availability: "available" }],
      ]),
      settings: {
        auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/p",
        accounts: [{ id: "a-1", agent: "codex", name: "Work", config_dir: "/home/me/.codex-work" }],
      },
    },
    { codex_account_id: "a-1" },
  );
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  expect(pickerNamed("Agent").textContent).toContain("Codex");
  expect(pickerNamed("Codex account").textContent).toContain("Work");

  const open = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Open");
  await act(async () => open?.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", agent: "codex", account: "a-1" }));
  act(() => root.unmount());
  container.remove();
});

it("opens a project session on the console's project session agent when the project names none", () => {
  const { container, root } = renderDialog([], {}, { default_agent: "claude", project_session_agent: "grok" });
  expect(pickerNamed("Agent").textContent).toContain("Grok Build");
  act(() => root.unmount());
  container.remove();
});

it("opens a console session on the console session agent whatever its project session agent is", () => {
  const { container, root } = renderDialog([], {}, { default_agent: "codex", project_session_agent: "grok" }, { kind: "unbound" }, null);
  expect(pickerNamed("Agent").textContent).toContain("Codex");
  act(() => root.unmount());
  container.remove();
});

it("leaves the agents known to be missing out of the agent picker", async () => {
  const { container, root } = renderDialog([], {
    agentAvailability: new Map<Agent, AgentAvailability>([
      ["claude", { agent: "claude", availability: "available" }],
      ["codex", { agent: "codex", availability: "unavailable" }],
      ["grok", { agent: "grok", availability: "available" }],
    ]),
  });
  await act(async () => pickerNamed("Agent").click());
  expect(optionElements().map((o) => o.textContent)).toEqual(["Claude Code", "Grok Build"]);
  act(() => root.unmount());
  container.remove();
});

it("lists only the added agents in the agent picker, in the order they were added", async () => {
  const { container, root } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["grok", "claude"], default_clone_dir: "/p", accounts: [] },
  });
  await act(async () => pickerNamed("Agent").click());
  expect(optionElements().map((o) => o.textContent)).toEqual(["Grok Build", "Claude Code"]);
  act(() => root.unmount());
  container.remove();
});

it("opens on the first added agent when the console's default agent is not added", () => {
  const { container, root } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["codex"], default_clone_dir: "/p", accounts: [] },
  });
  expect(pickerNamed("Agent").textContent).toContain("Codex");
  act(() => root.unmount());
  container.remove();
});

it("closes and reports the same alert when agents are installed but none is added", () => {
  const { container, root, onClose, daemon } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/p", accounts: [] },
  });
  expect(onClose).toHaveBeenCalled();
  expect(daemon.toastError).toHaveBeenCalledOnce();
  act(() => root.unmount());
  container.remove();
});

/** Opens the dialog with two stored Claude Code accounts, the console pinning one and the project
 * `projectPin`, presses Open and returns the `account` sent. */
async function openedAccount(projectPin: Partial<typeof project>): Promise<unknown> {
  const { container, root, daemon } = renderDialog(
    [],
    {
      settings: {
        auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/p",
        accounts: [
          { id: "a-console", agent: "claude", name: "Console pick", config_dir: "/home/me/.claude-console" },
          { id: "a-project", agent: "claude", name: "Project pick", config_dir: "/home/me/.claude-project" },
        ],
      },
    },
    { claude_account_id: "a-console" },
    { kind: "choose" },
    projectPin,
  );
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  const open = Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Open");
  await act(async () => open?.click());
  const sent = vi.mocked(daemon.request).mock.calls[0][0] as { account?: unknown };
  act(() => root.unmount());
  container.remove();
  return sent.account;
}

it("opens on the project's account for the agent, over the console's, and sends it", async () => {
  expect(await openedAccount({ claude_account_id: { kind: "account", id: "a-project" } })).toBe("a-project");
});

it("opens on the console's account when the project pins none", async () => {
  expect(await openedAccount({})).toBe("a-console");
});


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
  settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", accounts: [
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

it("shows the daemon's refusal of the conversation under the Session ID field and takes it away once the ID changes", async () => {
  const { daemon } = renderForResume();
  answerRequests(daemon, record("claude", null), () => Promise.reject(
    new DaemonRequestError("plain", "conversation_to_resume_not_found", { agent: "Claude Code", path: "/Users/dev/.claude", id: UUID }),
  ));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit();
  const field = resumeField()!;
  expect(field.getAttribute("aria-invalid")).toBe("true");
  expect(document.activeElement).toBe(field);
  const refusal = field.closest('[data-slot="textfield"], [role="group"]')?.textContent ?? "";
  expect(refusal).toContain("was found in /Users/dev/.claude");
  // Under the field, not in the alert at the dialog's foot.
  expect(document.body.querySelectorAll('[role="alert"]')).toHaveLength(1);
  typeResumeId("11111111-2222-3333-4444-555555555556");
  expect(document.body.textContent).not.toContain("was found in");
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

const archived = sessionOf("s-old", parentConsole.id, project.id, "Old work", "archived", { account_id: "a-work" });
const running = sessionOf("s-run", parentConsole.id, project.id, "Busy", "working");
const interrupted = sessionOf("s-int", parentConsole.id, project.id, "Paused", "interrupted", { account_id: "a-work" });

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
  expect(resumeField()!.getAttribute("aria-invalid")).toBe("true");
  expect(openDisabled()).toBe(true);
});

it("says the agent that holds the conversation is no longer offered, and never opens the pickers' fallback", async () => {
  const { daemon } = renderForResume({}, record("codex", null), { settings: { ...claudeAccounts.settings!, added_agents: ["claude"] } });
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeField()!.closest('[data-slot="textfield"], [role="group"]')?.textContent).toContain("no longer available");
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
  { name: "belongs to a session outside the project", reply: heldBy("outside_project", "s-unknown"), message: "already belongs to a session outside this project" },
  { name: "is in no account", reply: heldBy("not_found"), message: "No conversation with this ID was found in any account." },
])("says under the field that the conversation $name, and holds Open back", async ({ reply, message }) => {
  const { daemon } = renderForResume({}, reply);
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  const field = resumeField()!;
  expect(field.getAttribute("aria-invalid")).toBe("true");
  expect(field.closest('[data-slot="textfield"], [role="group"]')?.textContent).toContain(message);
  expect(openDisabled()).toBe(true);
  await submit();
  expect(sentOpenSession(daemon)).toBeUndefined();
});

const otherProject = projectOf("p-2", parentConsole.id, "Billing");
const elsewhere = sessionOf("s-else", parentConsole.id, otherProject.id, "Invoices", "idle");
const consoleHeld = sessionOf("s-cons", parentConsole.id, undefined, "Hub work", "idle");

it.each([
  { name: "a session of another project", holder: elsewhere, message: `This conversation is already held by the session ${isolate("Invoices")} in ${isolate("Billing")}.` },
  { name: "a console session", holder: consoleHeld, message: `This conversation is already held by the console session ${isolate("Hub work")} in ${isolate("Main")}.` },
])("names $name that holds the conversation, and where", async ({ holder, message }) => {
  const initial = { sessions: new Map([[holder.id, holder]]), projects: new Map([[otherProject.id, otherProject]]), consoles: new Map([[parentConsole.id, parentConsole]]) };
  renderForResume({}, heldBy("outside_project", holder.id), initial);
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeField()!.closest('[data-slot="textfield"], [role="group"]')?.textContent).toContain(message);
  expect(openDisabled()).toBe(true);
  expect(nameInput()).toBeUndefined();
});

it("names the holder in the open-time refusal too, when the store knows the session", async () => {
  const { daemon } = renderForResume({}, record("claude", null), { sessions: new Map([[elsewhere.id, elsewhere]]), projects: new Map([[otherProject.id, otherProject]]) });
  answerRequests(daemon, record("claude", null), (body) =>
    body.type === "open_session" ? Promise.reject(new DaemonRequestError("plain", "conversation_outside_project", { id: UUID, session: elsewhere.id })) : Promise.resolve({ type: "ack" }));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  await submit();
  expect(resumeField()!.closest('[data-slot="textfield"], [role="group"]')?.textContent).toContain(`held by the session ${isolate("Invoices")} in ${isolate("Billing")}`);
});

it("shows a failed lookup under the field with the daemon's reason, and holds Open back", async () => {
  const { daemon } = renderForResume({}, () => Promise.reject(new DaemonRequestError("plain", "unknown_project", { id: "p-1" })));
  await chooseResume();
  typeResumeId(UUID);
  await settle();
  expect(resumeField()!.getAttribute("aria-invalid")).toBe("true");
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
  expect(resumeField()!.getAttribute("aria-invalid")).not.toBe("true");
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

it("updates the fixed owner's displayed name while the session dialog is open", () => {
  const owner = { ...hub1, terminal_title: "First task" };
  const { container, root, daemon } = renderDialog([owner], { sessions: new Map([[owner.id, owner]]) }, {}, { kind: "bound", to: owner });
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  expect(document.body.textContent).toContain("First task");
  act(() => daemon.store.setState({ sessions: new Map([[owner.id, { ...owner, terminal_title: "Next task" }]]) }));
  expect(document.body.textContent).toContain(`${isolate("Next task")}｜${isolate(owner.title)}`);
  expect(document.body.textContent).not.toContain("First task");
});


const pickerNamed = (name: string) => buttons().find((button) =>
  button.getAttribute("aria-label") === name ||
  button.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.textContent === name),
)!;

function answerSessionModels(daemon: Daemon) {
  vi.mocked(daemon.request).mockImplementation(async (body) => body.type === "list_agent_models"
    ? { type: "agent_models", agent: body.agent, account: body.account ?? null, models: [
      { id: "opus", name: "Opus", efforts: ["low", "high"] },
      { id: "sonnet", name: "Sonnet", efforts: ["low", "high"] },
    ], unavailable: null }
    : { type: "ack" });
}

it.each(["inherit", "pick", "clear"] as const)("opens with session launch settings: %s", async (mode) => {
  const { container, root, daemon } = renderDialog([], {}, {}, { kind: "unbound" }, {
    claude_session_model: "opus", claude_session_effort: "high",
  });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "list_agent_models")).toBe(false);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain("Opus");
  expect(pickerNamed("Reasoning effort").textContent).toContain("high");
  if (mode === "pick") {
    await act(async () => pickerNamed("Model").click());
    await act(async () => optionElements().find((option) => option.textContent === "Sonnet")!.click());
    await act(async () => pickerNamed("Reasoning effort").click());
    await act(async () => optionElements().find((option) => option.textContent === "low")!.click());
  } else if (mode === "clear") {
    await act(async () => pickerNamed("Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
  }
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  const sent = vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")![0];
  if (mode === "inherit") {
    expect(sent).not.toHaveProperty("model");
    expect(sent).not.toHaveProperty("effort");
  } else if (mode === "pick") {
    expect(sent).toMatchObject({ model: "sonnet", effort: "low" });
  } else {
    expect(sent).toMatchObject({ model: null });
    expect(sent).not.toHaveProperty("effort");
  }
});

it("queries the selected account and keeps overrides separate when changing agents", async () => {
  const { container, root, daemon, pastMountRefocus } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", accounts: [
      { id: "a-work", agent: "claude", name: "Work", config_dir: "/accounts/work" },
    ] },
  }, { claude_account_id: "a-work" }, { kind: "unbound" }, { codex_session_model: "codex-project" });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await pastMountRefocus();
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(daemon.request).toHaveBeenCalledWith({ type: "list_agent_models", agent: "claude", account: "a-work" });
  await act(async () => pickerNamed("Model").click());
  await act(async () => optionElements().find((option) => option.textContent === "Sonnet")!.click());
  await act(async () => pickerNamed("Claude Code account").click());
  expect(optionElements().map((option) => option.textContent)).toEqual(["Default", "Work"]);
  await act(async () => optionElements().find((option) => option.textContent === "Default")!.click());
  expect(daemon.request).toHaveBeenCalledWith({ type: "list_agent_models", agent: "claude" });
  const agentTrigger = pickerNamed("Agent");
  await act(async () => { agentTrigger.focus(); agentTrigger.click(); });
  await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "codex")!.click());
  await act(async () => {
    await vi.waitFor(() => expect(document.activeElement).toBe(agentTrigger));
  });
  expect(pickerNamed("Agent")).toBe(agentTrigger);
  expect(daemon.request).toHaveBeenCalledWith({ type: "list_agent_models", agent: "codex" });
  expect(pickerNamed("Model").textContent).toContain("codex-project");
  expect(pickerNamed("Codex account").textContent).toContain("Default");
  await act(async () => pickerNamed("Agent").click());
  await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "claude")!.click());
  expect(pickerNamed("Model").textContent).toContain("Sonnet");
  expect(pickerNamed("Claude Code account").textContent).toContain("Default");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  const sent = vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")![0];
  expect(sent).toMatchObject({ agent: "claude", account: null, model: "sonnet" });
});


it("keeps the selected agent and the account picker when the last named account is removed", async () => {
  const { container, root, daemon, pastMountRefocus } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", accounts: [
      { id: "a-work", agent: "claude", name: "Work", config_dir: "/accounts/work" },
    ] },
  }, { default_agent: "codex" });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await pastMountRefocus();
  await act(async () => pickerNamed("Agent").click());
  await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "claude")!.click());
  await act(async () => pickerNamed("Claude Code account").click());
  await act(async () => optionElements().find((option) => option.textContent === "Work")!.click());
  const agentTrigger = pickerNamed("Agent");
  act(() => daemon.store.setState({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", accounts: [] } }));
  expect(pickerNamed("Agent")).toBe(agentTrigger);
  expect(agentTrigger.textContent).toContain("Claude Code");
  expect(pickerNamed("Claude Code account").textContent).toContain("Default");
});


it.each(["inherit", "pick", "clear"] as const)("opens console sessions with console launch settings: %s", async (mode) => {
  const { container, root, daemon } = renderDialog([hub1], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", accounts: [
      { id: "a-console", agent: "claude", name: "Console account", config_dir: "/accounts/console" },
      { id: "a-codex", agent: "codex", name: "Codex work", config_dir: "/accounts/codex" },
    ] },
  }, {
    claude_account_id: "a-console", claude_console_session_model: "opus", claude_console_session_effort: "high",
    codex_account_id: "a-codex", codex_console_session_model: "codex-console", codex_console_session_effort: "medium",
  }, { kind: "choose" }, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  expect(document.body.querySelector('[role="dialog"] h2')?.textContent).toBe(`Open console session in ${isolate("Main")}`);
  expect(document.body.textContent).not.toContain("Report to");
  const name = document.body.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  expect(name.placeholder).toBe(`${parentConsole.name} session`);
  expect(pickerNamed("Agent").textContent).toContain("Claude Code");
  expect(pickerNamed("Claude Code account").textContent).toContain("Console account");
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "list_agent_models")).toBe(false);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain("Opus");
  expect(pickerNamed("Reasoning effort").textContent).toContain("high");
  expect(document.body.textContent).toContain("Unchanged values follow the console settings.");
  if (mode === "pick") {
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(name, "Release coordination");
      name.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await act(async () => pickerNamed("Agent").click());
    await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "codex")!.click());
    expect(pickerNamed("Codex account").textContent).toContain("Codex work");
    expect(pickerNamed("Model").textContent).toContain("codex-console");
    expect(pickerNamed("Reasoning effort").textContent).toContain("medium");
    await act(async () => pickerNamed("Codex account").click());
    await act(async () => optionElements().find((option) => option.textContent === "Default")!.click());
    await act(async () => pickerNamed("Model").click());
    await act(async () => optionElements().find((option) => option.textContent === "Sonnet")!.click());
    await act(async () => pickerNamed("Reasoning effort").click());
    await act(async () => optionElements().find((option) => option.textContent === "low")!.click());
  } else if (mode === "clear") {
    await act(async () => pickerNamed("Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
    await act(async () => pickerNamed("Reasoning effort").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
  }
  expect(vi.mocked(daemon.request).mock.calls.some(([body]) => body.type === "open_session")).toBe(false);
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  const sent = vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")![0];
  expect(sent).not.toHaveProperty("project_id");
  expect(sent).not.toHaveProperty("bound_to");
  if (mode === "inherit") {
    expect(sent).toMatchObject({ agent: "claude", account: "a-console" });
    expect(sent).not.toHaveProperty("model");
    expect(sent).not.toHaveProperty("effort");
    expect(sent).toHaveProperty("title", "Main session");
  } else if (mode === "pick") {
    expect(sent).toMatchObject({ title: "Release coordination", agent: "codex", account: null, model: "sonnet", effort: "low" });
  } else {
    expect(sent).toMatchObject({ model: null, effort: null });
  }
});

it("opens console sessions on the console's default agent and its account", async () => {
  const { container, root, daemon } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "", accounts: [
      { id: "a-codex", agent: "codex", name: "Work", config_dir: "/accounts/codex" },
    ] },
  }, { default_agent: "codex", codex_account_id: "a-codex" }, undefined, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" });
  expect(pickerNamed("Agent").textContent).toContain("Codex");
  expect(pickerNamed("Codex account").textContent).toContain("Work");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ agent: "codex", account: "a-codex" }));
});


it("uses the localized default name as its placeholder and submitted title", async () => {
  const { container, root, daemon } = renderDialog([], {}, {}, undefined, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const name = document.body.querySelector<HTMLInputElement>('[role="dialog"] input')!;
  expect(name.placeholder).toBe(`${parentConsole.name} session`);
  expect(name.value).toBe("");
  expect(daemon.request).not.toHaveBeenCalledWith(expect.objectContaining({ type: "preview_console_session_title" }));
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", title: "Main session" }));
});

it.each(["Backspace", "Delete"])("clears a launch choice with %s only while its list is closed", async (key) => {
  const { container, root, daemon, pastMountRefocus } = renderDialog([], {}, {}, undefined, { claude_session_model: "opus" });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await pastMountRefocus();
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  const trigger = pickerNamed("Model");
  await act(async () => { trigger.focus(); trigger.click(); });
  await act(async () => { await vi.waitFor(() => expect(document.activeElement?.getAttribute("role")).toBe("option")); });
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })));
  expect(trigger.textContent).toContain("Opus");
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await act(async () => { await vi.waitFor(() => expect(document.activeElement).toBe(trigger)); });
  await act(async () => trigger.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true })));
  expect(trigger.textContent).toContain("The agent's own setting");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", model: null }));
});

it.each([
  ["the project's own value over the console's project session one", { claude_session_model: "opus" }, { claude_project_session_model: "sonnet" }, "Opus", "The agent's own setting"],
  ["the console's project session values where the project sets none", {}, { claude_project_session_model: "sonnet", claude_project_session_effort: "low" }, "Sonnet", "low"],
  ["each value from the project, else from the console", { claude_session_effort: "high" }, { claude_project_session_model: "sonnet", claude_project_session_effort: "low" }, "Sonnet", "high"],
] as const)("opens a project session on %s", async (_, projectValues, consoleValues, model, effort) => {
  const { container, root, daemon } = renderDialog([], {}, consoleValues, { kind: "unbound" }, projectValues);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain(model);
  expect(pickerNamed("Reasoning effort").textContent).toContain(effort);
});

it("opens a console session on the console session value, not the project session one", async () => {
  const { container, root, daemon } = renderDialog([], {}, { claude_console_session_model: "opus", claude_project_session_model: "sonnet" }, { kind: "unbound" }, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  answerSessionModels(daemon);
  await act(async () => buttons().find((button) => button.textContent === "Advanced")!.click());
  expect(pickerNamed("Model").textContent).toContain("Opus");
});

it("keeps the Name hint visible", () => {
  const { container, root } = renderDialog([], {}, {}, undefined, null);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  expect(document.body.textContent).toContain("Leave this empty to use the default session name.");
});

it("shows the account picker with the agent's only account, its usage drawn in the option", () => {
  const { container, root } = renderDialog([], {
    accountQuotas: [{
      agent: "claude", account: null, fetched_at: Date.now(), plan: null, unavailable: null,
      windows: [{ kind: "session", label: null, used_percent: 30, resets_at: Date.now() + 3_600_000, window_minutes: 300 }],
    }],
  });
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const account = pickerNamed("Claude Code account");
  expect(account).toBeDefined();
  expect(pickerNamed("Agent").getAttribute("aria-describedby")).toBeNull();
  // The only bar is the one in the account picker's own trigger.
  expect(document.body.querySelectorAll('[class*="quota-meter--"]')).toHaveLength(1);
});
