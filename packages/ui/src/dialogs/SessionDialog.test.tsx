// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { expect, it, onTestFinished, vi } from "vitest";

import { isolate } from "../i18n/isolate";
import type { Agent, AgentAvailability, Session } from "../protocol";
import type { NewSessionBinding } from "./dialogRequest";
import { sessionOf } from "../gallery/fixtures/builders";
import { DaemonProvider } from "../store";
import { SessionDialog } from "./SessionDialog";
import { parentConsole, project, renderDialog, hub1, buttons, optionElements, choose, pickerNamed } from "./sessionDialogTestSupport";
import { DEFAULT_GIT_WORKFLOW } from "../gitWorkflow";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

/** An `agentAvailability` map with every agent `unavailable` — "no agent available at all". Named
 * distinctly from `agents.ts`'s own `noAgentState`, which this is only a fixture for. */
function everyAgentUnavailable(): Map<Agent, AgentAvailability> {
  const agents: Agent[] = ["claude", "codex", "grok"];
  return new Map(agents.map((agent) => [agent, { agent, availability: "unavailable" as const }]));
}

const hub2 = sessionOf("s-hub-2", parentConsole.id, undefined, "Hub 2", "working", { colour: "pink", started_at: 200 });
const hubArchived = sessionOf("s-hub-3", parentConsole.id, undefined, "Hub 3", "archived", { colour: "cyan" });

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

it.each([
  { name: "defaults to none and offers every live console session", sessions: [hub1, hub2, hubArchived], pick: undefined, binding: choose, options: ["No console session", "Hub 2", "Hub"], boundTo: undefined },
  { name: "binds to the console session chosen", sessions: [hub1, hub2, hubArchived], pick: "Hub", binding: choose, options: ["No console session", "Hub 2", "Hub"], boundTo: "s-hub-1" },
  { name: "offers no choice with no live console session", sessions: [hubArchived], pick: undefined, binding: choose, options: [], boundTo: undefined },
  { name: "offers no choice and sends no binding when unbound", sessions: [hub1, hub2], pick: undefined, binding: { kind: "unbound" } as const, options: [], boundTo: undefined },
  { name: "binds to the fixed owner without offering a choice", sessions: [hub1, hub2], pick: undefined, binding: { kind: "bound", to: hub1 } as const, options: [], boundTo: "s-hub-1", fixedLine: "Reports to Hub" },
])("$name", async ({ sessions, pick, binding, options, boundTo, fixedLine }) => {
  const result = await openWith(sessions, pick, binding);
  expect(result.options).toEqual(options);
  expect(result.request.bound_to).toBe(boundTo);
  expect(result.text.includes("Reports to")).toBe(fixedLine !== undefined);
  if (fixedLine) expect(result.text).toContain(fixedLine);
});

it.each(["archived", "removed"])("turns a fixed-owner form into a failure modal with the structured message when its owner is %s", async (change) => {
  const { container, root, daemon, onClose, onOpened, onResume } = renderDialog([hub1], {}, {}, { kind: "bound", to: hub1 });
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  expect(onClose).not.toHaveBeenCalled();
  expect(daemon.toastError).not.toHaveBeenCalled();
  act(() => root.render(
    <DaemonProvider value={daemon}>
      <SessionDialog
        console={parentConsole}
        project={project}
        binding={{ kind: "bound", to: hub1 }}
        sessions={change === "archived" ? [{ ...hub1, status: "archived" }] : []}
        onClose={onClose}
        onOpened={onOpened}
        onResume={onResume}
      />
    </DaemonProvider>,
  ));
  expect(onClose).not.toHaveBeenCalled();
  expect(daemon.toastError).not.toHaveBeenCalled();
  const dialog = document.body.querySelector('[role="alertdialog"]');
  expect(dialog?.textContent).toContain("Hub is no longer available, so a session cannot be opened for it.");
  expect(dialog?.querySelector("form")).toBeNull();
  expect(Array.from(dialog?.querySelectorAll("button") ?? []).map((button) => button.textContent)).toEqual(["OK"]);
  await act(async () => buttons().find((b) => b.textContent === "OK")?.click());
  expect(onClose).toHaveBeenCalledTimes(1);
  expect(daemon.request).not.toHaveBeenCalled();
});

it("says only the owner's loss when both the fixed owner and every agent are unavailable", () => {
  const { container, root, daemon, onClose } = renderDialog([], { agentAvailability: everyAgentUnavailable() }, {}, { kind: "bound", to: hub1 });
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  expect(onClose).not.toHaveBeenCalled();
  expect(daemon.toastError).not.toHaveBeenCalled();
  const dialog = document.body.querySelector('[role="alertdialog"]');
  expect(dialog?.textContent).toContain("is no longer available");
  expect(dialog?.textContent).not.toContain("No agent is available");
});

it("keeps the owner select, back on none, when the chosen console session is archived under it", async () => {
  const { container, root, daemon, onClose } = renderDialog([hub1]);
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
          onClose={onClose}
          onResume={async () => {}}
          onOpened={() => {}}
        />
      </DaemonProvider>,
    ),
  );
  expect(onClose).not.toHaveBeenCalled();
  expect(daemon.toastError).not.toHaveBeenCalled();
  const ownerTrigger = buttons().find((b) => b.textContent?.includes("No console session"));
  expect(ownerTrigger).toBeDefined();
  await act(async () => ownerTrigger?.click());
  expect(optionElements().map((o) => o.textContent)).toEqual(["No console session"]);

  await act(async () => buttons().find((b) => b.textContent === "Open")?.click());
  expect(vi.mocked(daemon.request).mock.calls[0][0]).toMatchObject({ type: "open_session", bound_to: undefined });
  act(() => root.unmount());
  container.remove();
});

it("turns into an acknowledgement-only failure modal when no agent is available at all", () => {
  const { container, root, onClose, daemon } = renderDialog([], { agentAvailability: everyAgentUnavailable() });
  expect(onClose).not.toHaveBeenCalled();
  expect(daemon.toastError).not.toHaveBeenCalled();
  expect(document.body.querySelector('[role="alertdialog"]')?.textContent).toContain("No agent is available to open a session.");
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
        auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW,
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
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", agent: "codex", account: "a-1", terminal_appearance: "dark" }));
  act(() => root.unmount());
  container.remove();
});

it("opens on the account whose weekly quota resets soonest when automatic selection is on", async () => {
  const now = Date.now();
  const weekly = (used_percent: number, resets_at: number) => ({ kind: "weekly" as const, label: null, used_percent, resets_at, window_minutes: 7 * 24 * 60 });
  const { container, root, daemon } = renderDialog(
    [],
    {
      settings: {
        auto_pull_repositories: false, auto_delete_unresumable_archives: true, auto_select_account: true, added_agents: null, default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW,
        accounts: [
          { id: "a-1", agent: "codex", name: "Work", config_dir: "/home/me/.codex-work" },
          { id: "a-soon", agent: "codex", name: "Soon", config_dir: "/home/me/.codex-soon" },
        ],
      },
      accountQuotas: [
        { agent: "codex", account: "a-1", fetched_at: now, plan: null, unavailable: null, windows: [weekly(10, now + 3 * 24 * 60 * 60 * 1000)] },
        { agent: "codex", account: "a-soon", fetched_at: now, plan: null, unavailable: null, windows: [weekly(99, now + 60 * 60 * 1000)] },
      ],
    },
    { default_agent: "codex", codex_account_id: "a-1" },
  );
  vi.mocked(daemon.request).mockResolvedValue({ type: "ack" } as never);
  expect(pickerNamed("Codex account").textContent).toContain("Soon");
  await act(async () => buttons().find((button) => button.textContent === "Open")?.click());
  expect(daemon.request).toHaveBeenCalledWith(expect.objectContaining({ type: "open_session", agent: "codex", account: "a-soon" }));
  act(() => root.unmount());
  container.remove();
});

it("hides turned-off accounts and hides their agent when every account is off", async () => {
  const settings = {
    auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW, disabled_default_accounts: ["codex" as const],
    accounts: [
      { id: "a-1", agent: "codex" as const, name: "Work", config_dir: "/home/me/.codex-work", disabled: true },
      { id: "a-2", agent: "codex" as const, name: "Spare", config_dir: "/home/me/.codex-spare" },
    ],
  };
  const first = renderDialog([], { settings }, { default_agent: "codex" });
  expect(pickerNamed("Codex account").textContent).toContain("Spare");
  await act(async () => pickerNamed("Codex account").click());
  expect(optionElements().map((option) => option.textContent)).toEqual(["Spare"]);
  act(() => first.root.unmount());
  first.container.remove();

  const second = renderDialog([], { settings: { ...settings, accounts: settings.accounts.map((account) => ({ ...account, disabled: true })) } }, { default_agent: "codex" });
  expect(pickerNamed("Agent").textContent).toContain("Claude Code");
  await act(async () => pickerNamed("Agent").click());
  expect(optionElements().map((option) => option.textContent)).toEqual(["Claude Code", "Grok Build"]);
  act(() => second.root.unmount());
  second.container.remove();
});

it("drops an agent whose last enabled account is turned off while the dialog is open, and offers it again when enabled", async () => {
  const view = renderDialog([], {}, { default_agent: "codex" });
  onTestFinished(() => {
    act(() => view.root.unmount());
    view.container.remove();
  });
  expect(pickerNamed("Agent").textContent).toContain("Codex");
  act(() => view.daemon.store.setState((state) => ({ settings: { ...state.settings, disabled_default_accounts: ["codex"] } })));
  expect(pickerNamed("Agent").textContent).toContain("Claude Code");
  await act(async () => pickerNamed("Agent").click());
  expect(optionElements().map((option) => option.textContent)).toEqual(["Claude Code", "Grok Build"]);
  act(() => view.daemon.store.setState((state) => ({ settings: { ...state.settings, disabled_default_accounts: [] } })));
  expect(optionElements().map((option) => option.textContent)).toEqual(["Claude Code", "Codex", "Grok Build"]);
});

it("refuses opening a session when every agent's accounts are turned off", () => {
  const view = renderDialog([]);
  onTestFinished(() => {
    act(() => view.root.unmount());
    view.container.remove();
  });
  act(() => view.daemon.store.setState((state) => ({ settings: { ...state.settings, disabled_default_accounts: ["claude", "codex", "grok"] } })));
  expect(document.body.querySelector('[role="alertdialog"]')?.textContent).toContain("No agent is available to open a session.");
  expect(pickerNamed("Agent")).toBeUndefined();
  expect(view.daemon.request).not.toHaveBeenCalled();
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
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["grok", "claude"], default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [] },
  });
  await act(async () => pickerNamed("Agent").click());
  expect(optionElements().map((o) => o.textContent)).toEqual(["Grok Build", "Claude Code"]);
  act(() => root.unmount());
  container.remove();
});

it("opens on the first added agent when the console's default agent is not added", () => {
  const { container, root } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: ["codex"], default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [] },
  });
  expect(pickerNamed("Agent").textContent).toContain("Codex");
  act(() => root.unmount());
  container.remove();
});

it("turns into the same failure modal when agents are installed but none is added", () => {
  const { container, root, onClose, daemon } = renderDialog([], {
    settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: [], default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW, accounts: [] },
  });
  expect(onClose).not.toHaveBeenCalled();
  expect(daemon.toastError).not.toHaveBeenCalled();
  expect(document.body.querySelector('[role="alertdialog"]')?.textContent).toContain("No agent is available to open a session.");
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
        auto_pull_repositories: false, auto_delete_unresumable_archives: true, added_agents: null, default_clone_dir: "/p", git_workflow: DEFAULT_GIT_WORKFLOW,
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
