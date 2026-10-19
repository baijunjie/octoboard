// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { DaemonRequestError } from "../daemon-client";
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
 * distinctly from `agents.ts`'s own `noAgentAvailable`, which this is only a fixture for. */
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
): { container: HTMLElement; root: ReturnType<typeof createRoot>; daemon: Daemon; pastMountRefocus: () => Promise<void> } {
  const daemon = fakeDaemon(initial);
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
          onClose={() => {}}
          onOpened={() => {}}
        />
      </DaemonProvider>,
    ),
  );
  return { container, root, daemon, pastMountRefocus: trackMountRefocus() };
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

it("shows the install prompt when no agent is available at all", () => {
  // HeroUI's `Modal` renders into a portal at `document.body`, not into the container itself.
  const { container, root } = renderDialog([], { agentAvailability: everyAgentUnavailable() });
  expect(document.body.textContent).toContain("Make sure one of them is on your PATH to open a session.");
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
        auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "/p",
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

/** Opens the dialog with two stored Claude Code accounts, the console pinning one and the project
 * `projectPin`, presses Open and returns the `account` sent. */
async function openedAccount(projectPin: Partial<typeof project>): Promise<unknown> {
  const { container, root, daemon } = renderDialog(
    [],
    {
      settings: {
        auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "/p",
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

/** Renders the dialog for a project (or, with `null`, for the console) over one console session and
 * mocks its request to succeed. */
function renderForResume(projectOverride: Partial<typeof project> | null = {}) {
  const { container, root, daemon } = renderDialog([hub1], {}, {}, { kind: "choose" }, projectOverride);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  return daemon;
}

const submit = async () => act(async () => buttons().find((button) => button.textContent === "Open")!.click());
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
  const daemon = renderForResume();
  expect(newTab()!.getAttribute("aria-selected")).toBe("true");
  expect(resumeField()).toBeUndefined();
  await submit();
  expect(sentOpenSession(daemon)).not.toHaveProperty("resume_agent_session_id");
});

it("sends the conversation to resume, trimmed, with no owner", async () => {
  const daemon = renderForResume();
  await chooseResume();
  expect(document.body.textContent).not.toContain("Report to");
  typeResumeId(`  ${UUID} `);
  await submit();
  expect(sentOpenSession(daemon)).toMatchObject({ resume_agent_session_id: UUID, bound_to: undefined });
});

it.each([
  { name: "empty", id: "", message: "Enter the session ID of the conversation to resume." },
  { name: "not a UUID", id: "my-title", message: "Enter a session ID like" },
])("holds back a $name conversation ID and says so under the field", async ({ id, message }) => {
  const daemon = renderForResume();
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
  const daemon = renderForResume();
  vi.mocked(daemon.request).mockRejectedValue(
    new DaemonRequestError("plain", "conversation_to_resume_not_found", { agent: "Claude Code", path: "/Users/dev/.claude", id: UUID }),
  );
  await chooseResume();
  typeResumeId(UUID);
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

it("brings back the owner choice and sends no conversation after choosing a new one again", async () => {
  const daemon = renderForResume();
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
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts: [
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
  expect(buttons().some((button) => button.getAttribute("aria-label") === "Codex account")).toBe(false);
  await act(async () => pickerNamed("Agent").click());
  await act(async () => optionElements().find((option) => option.getAttribute("data-key") === "claude")!.click());
  expect(pickerNamed("Model").textContent).toContain("Sonnet");
  expect(pickerNamed("Claude Code account").textContent).toContain("Default");
  await act(async () => buttons().find((button) => button.textContent === "Open")!.click());
  const sent = vi.mocked(daemon.request).mock.calls.find(([body]) => body.type === "open_session")![0];
  expect(sent).toMatchObject({ agent: "claude", account: null, model: "sonnet" });
});


it.each(["trigger", "popover"] as const)("keeps the selected agent and hands focus back when the last named account is removed from %s", async (focused) => {
  const { container, root, daemon, pastMountRefocus } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts: [
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
  act(() => pickerNamed("Claude Code account").focus());
  if (focused === "popover") {
    await act(async () => pickerNamed("Claude Code account").click());
    await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
    expect(document.activeElement?.getAttribute("role")).toBe("option");
  }
  act(() => daemon.store.setState({ settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts: [] } }));
  expect(pickerNamed("Agent")).toBe(agentTrigger);
  expect(agentTrigger.textContent).toContain("Claude Code");
  await act(() => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())));
  expect(document.activeElement).toBe(agentTrigger);
});


it.each(["inherit", "pick", "clear"] as const)("opens console sessions with console launch settings: %s", async (mode) => {
  const { container, root, daemon } = renderDialog([hub1], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts: [
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
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts: [
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
