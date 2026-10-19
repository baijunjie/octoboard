import { errorAlerts } from "../errorAlerts";
// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { Account, Agent, AgentAvailability, AgentModel, Console, Event, ModelsUnavailable, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { imageToAvatar } from "../avatarImage";
import { ConsoleDialog } from "./ConsoleDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Decoding an image needs a canvas, which jsdom does not have; each test says what the read gives.
vi.mock("../avatarImage", () => ({ imageToAvatar: vi.fn() }));

const work: Account = { id: "a-work", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" };
const personal: Account = { id: "a-personal", agent: "claude", name: "Personal", config_dir: "/home/dev/.claude-personal" };

const MODELS: AgentModel[] = [
  { id: "alpha", name: "Alpha", efforts: ["low", "high"] },
  { id: "beta", name: "Beta", efforts: ["medium", "high", "max"] },
  { id: "plain", name: "Plain", efforts: [] },
];

/** The reply to a `list_agent_models` request: `models` as listed, or why there are none. */
const listing = (body: RequestBody, models: AgentModel[], unavailable: ModelsUnavailable | null = null): Event => ({
  type: "agent_models",
  agent: (body as { agent: "claude" }).agent,
  account: (body as { account?: string }).account ?? null,
  models,
  unavailable,
});

/** The store of the daemon the last `render` set up, for a test that changes the state under it. */
let lastStore: ReturnType<typeof createStateStore>;

/** Renders the dialog over a daemon that answers every request, with `MODELS` unless `respond` says
 * otherwise, and returns the requests sent. */
function render(
  editing?: Console,
  accounts: Account[] = [],
  respond?: (body: RequestBody) => Promise<Event>,
  availability: Partial<Record<Agent, AgentAvailability["availability"]>> = {},
) {
  const request = vi.fn((body: RequestBody) => respond?.(body) ?? Promise.resolve(body.type === "list_agent_models" ? listing(body, MODELS) : ({ type: "ack" } as Event)));
  const daemon: Daemon = {
    store: createStateStore({
      settings: { auto_pull_repositories: false, auto_delete_unresumable_archives: true, default_clone_dir: "", accounts },
      agentAvailability: new Map(
        (["claude", "codex", "grok"] as Agent[]).map((agent) => [agent, { agent, availability: availability[agent] ?? "not_determined" }]),
      ),
    }),
    request,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  lastStore = daemon.store;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <ConsoleDialog console={editing} onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
  return request;
}

const PLACEHOLDER = "The agent's own setting";
const buttons = () => Array.from(document.body.querySelectorAll("button"));
const click = (el: Element | null | undefined) => act(async () => (el as HTMLElement | undefined)?.click());
const clickButton = (text: string | ((b: HTMLButtonElement) => boolean)) =>
  click(buttons().find((b) => (typeof text === "string" ? b.textContent === text : text(b))));
const choose = (option: string) =>
  click(Array.from(document.body.querySelectorAll('[role="option"]')).find((o) => o.textContent === option));
/** What the form sent apart from its questions about models and the quota refresh Settings' Agent accounts section
 * asks for when it opens over the dialog. */
const sent = (request: ReturnType<typeof render>) =>
  request.mock.calls.map((call) => call[0]).filter((body) => body.type !== "list_agent_models" && body.type !== "refresh_account_quotas");
const asked = (request: ReturnType<typeof render>) => request.mock.calls.map((call) => call[0]).filter((body) => body.type === "list_agent_models");

function type(label: string, value: string) {
  const input = Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find(
    (el) => el.labels?.[0]?.textContent === label,
  )!;
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Opens the Advanced section. The model picker is the first launch setting, the effort the second. */
async function openAdvanced() {
  await clickButton("Advanced");
}
/** The agent tab named `name` in the Advanced section. */
const clickTab = (name: string) => click(Array.from(document.body.querySelectorAll('[role="tab"]')).find((tab) => tab.textContent === name));
/** The name of `button` that comes from other elements, put together as `aria-labelledby` says (the
 * value shown in the button is part of the name too, and is left out). */
const nameOf = (button: Element) =>
  (button.getAttribute("aria-labelledby") ?? "")
    .split(" ")
    .map((id) => document.getElementById(id))
    .filter((el) => el && !button.contains(el))
    .map((el) => el!.textContent ?? "");
/** The Advanced picker of the setting `name` for the sessions of `group`, by the elements that name it. */
const groupPicker = (group: string, name: string) =>
  buttons().find((b) => {
    const [kind, setting] = nameOf(b);
    return kind === group && setting === name;
  })!;
/** The console session picker of the Advanced setting `name`. */
const pickerNamed = (name: string) => groupPicker("Console sessions", name);
const clearButton = (name: string) => pickerNamed(name).querySelector<HTMLElement>('[data-slot="select-clear-button"]');

it("creates without launch settings when none is set", async () => {
  const request = render();
  type("Name", "Main");
  await clickButton("Create");
  expect(sent(request)).toEqual([{ type: "create_console", name: "Main", default_agent: "claude" }]);
});

it("creates with the model and effort that were set", async () => {
  const request = render();
  type("Name", "Main");
  await openAdvanced();
  await click(pickerNamed("Model"));
  await choose("Alpha");
  await click(pickerNamed("Reasoning effort"));
  await choose("high");
  await clickButton("Create");
  expect(sent(request)).toEqual([
    {
      type: "create_console",
      name: "Main",
      default_agent: "claude",
      claude_console_session_model: "alpha",
      claude_console_session_effort: "high",
    },
  ]);
});

it("shows an unset picker as its placeholder", async () => {
  render();
  await openAdvanced();
  expect(pickerNamed("Model").textContent).toContain(PLACEHOLDER);
  expect(pickerNamed("Reasoning effort").textContent).toContain(PLACEHOLDER);
  // The note renders outside a HeroUI field, which its `Description` has to cope with.
  expect(document.body.textContent).toContain("Console sessions opened or resumed from now on use this model");
});

const stored = consoleOf("c-1", "Main", {
  claude_console_session_model: "alpha",
  claude_console_session_effort: "high",
  codex_console_session_model: "retired",
});

it("sends only what changed when editing", async () => {
  const request = render(stored);
  await openAdvanced();
  await click(pickerNamed("Reasoning effort"));
  await choose("low");
  await clickButton("Save");
  expect(sent(request)).toEqual([
    { type: "update_console", console: "c-1", name: "Main", default_agent: "claude", claude_console_session_effort: "low" },
  ]);
});

it("sends an explicit null when a picker is returned to unset", async () => {
  const request = render(stored);
  await openAdvanced();
  await click(clearButton("Model"));
  expect(pickerNamed("Model").textContent).toContain(PLACEHOLDER);
  await clickButton("Save");
  expect(sent(request)).toEqual([
    { type: "update_console", console: "c-1", name: "Main", default_agent: "claude", claude_console_session_model: null },
  ]);
});

it("never sends the fields of an agent that was not touched", async () => {
  // Codex holds a model of its own; a rename alone must leave it, and every other agent's, out.
  const request = render(stored);
  type("Name", "Renamed");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Renamed", default_agent: "claude" }]);
});

it("keeps an edit to an agent that is no longer the selected one", async () => {
  const request = render(consoleOf("c-1", "Main"), [work, personal]);
  await clickButton("Default");
  await choose("Personal");
  await clickButton((b) => b.textContent?.includes("Claude Code") ?? false);
  await choose("Codex");
  await clickButton("Save");
  expect(sent(request)).toEqual([
    { type: "update_console", console: "c-1", name: "Main", default_agent: "codex", claude_account_id: "a-personal" },
  ]);
});

it("leaves an account reference that was since removed alone unless its picker is used", async () => {
  const request = render(consoleOf("c-1", "Main", { codex_account_id: "a-gone" }), [work, personal]);
  type("Name", "Renamed");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Renamed", default_agent: "claude" }]);
});

const AVATAR = "data:image/png;base64,AAAA";
const withAvatar = consoleOf("c-1", "Main", { icon: AVATAR });
const button = (name: string) => buttons().find((b) => b.getAttribute("aria-label") === name);

/** Hands the avatar field's file input a file, as choosing one in the picker does. */
async function pickFile() {
  const input = document.body.querySelector<HTMLInputElement>('input[type="file"]')!;
  Object.defineProperty(input, "files", { configurable: true, value: [new File(["x"], "face.png", { type: "image/png" })] });
  await act(async () => void input.dispatchEvent(new Event("change", { bubbles: true })));
}

it("offers an upload and no delete button while there is no avatar", () => {
  render();
  expect(button("Choose image")).toBeDefined();
  expect(button("Remove image")).toBeUndefined();
});

it("shows the avatar with a delete button, and sends an explicit null once it is removed", async () => {
  const request = render(withAvatar);
  expect(document.body.querySelector("img")?.getAttribute("src")).toBe(AVATAR);
  expect(button("Replace image")).toBeDefined();
  await click(button("Remove image"));
  expect(document.body.querySelector("img")).toBeNull();
  expect(button("Choose image")).toBeDefined();
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "claude", icon: null }]);
});

it("creates with the avatar that was picked", async () => {
  vi.mocked(imageToAvatar).mockResolvedValue(AVATAR);
  const request = render();
  type("Name", "Main");
  await pickFile();
  expect(document.body.querySelector("img")?.getAttribute("src")).toBe(AVATAR);
  await clickButton("Create");
  expect(sent(request)).toEqual([{ type: "create_console", name: "Main", default_agent: "claude", icon: AVATAR }]);
});

it("reports an unreadable avatar through the acknowledgement queue and keeps the avatar", async () => {
  vi.mocked(imageToAvatar).mockRejectedValue(new Error("not an image"));
  render(withAvatar);
  await pickFile();
  expect(errorAlerts.getSnapshot().some((error) => error.message.includes("could not be read as an image"))).toBe(true);
  expect(document.body.querySelector("img")?.getAttribute("src")).toBe(AVATAR);
});

function typeInstructions(value: string) {
  const area = document.body.querySelector<HTMLTextAreaElement>("textarea")!;
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(area, value);
    area.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

it("creates with the console session instructions that were written", async () => {
  const request = render();
  type("Name", "Main");
  await openAdvanced();
  typeInstructions("Be brief.\n");
  await clickButton("Create");
  expect(sent(request)).toEqual([
    { type: "create_console", name: "Main", default_agent: "claude", console_session_prompt: "Be brief." },
  ]);
});

it("sends the console session instructions only when they changed, and an explicit null once cleared", async () => {
  const withInstructions = consoleOf("c-1", "Main", { console_session_prompt: "Be brief." });
  const request = render(withInstructions);
  await openAdvanced();
  expect(document.body.querySelector("textarea")?.value).toBe("Be brief.");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "claude" }]);
  typeInstructions("  ");
  await clickButton("Save");
  expect(sent(request)[1]).toEqual({
    type: "update_console",
    console: "c-1",
    name: "Main",
    default_agent: "claude",
    console_session_prompt: null,
  });
});

const optionTexts = () => Array.from(document.body.querySelectorAll('[role="option"]')).map((o) => o.textContent);
const locked = (name: string) => (pickerNamed(name) as HTMLButtonElement).disabled;

it("asks for the models only once Advanced is opened, and offers what comes back", async () => {
  const request = render();
  expect(asked(request)).toEqual([]);
  await openAdvanced();
  expect(asked(request)).toEqual([{ type: "list_agent_models", agent: "claude" }]);
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["Alpha", "Beta", "Plain"]);
});

it("keeps a stored model the list lacks, last and marked", async () => {
  render(consoleOf("c-1", "Main", { claude_console_session_model: "retired" }));
  await openAdvanced();
  expect(pickerNamed("Model").textContent).toContain("retired (not in the list)");
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["Alpha", "Beta", "Plain", "retired (not in the list)"]);
});

it("asks under the account that is settled, and again when the account or the agent changes", async () => {
  const request = render(consoleOf("c-1", "Main", { claude_account_id: "a-work" }), [work, personal]);
  await openAdvanced();
  await clickButton("Work");
  await choose("Personal");
  await clickTab("Codex");
  expect(asked(request)).toEqual([
    { type: "list_agent_models", agent: "claude", account: "a-work" },
    { type: "list_agent_models", agent: "claude", account: "a-personal" },
    { type: "list_agent_models", agent: "codex" },
  ]);
});

it("ignores a late answer for an agent the form has moved off", async () => {
  const replies: { body: RequestBody; resolve: (event: Event) => void }[] = [];
  render(undefined, [], (body) => new Promise((resolve) => replies.push({ body, resolve })));
  await openAdvanced();
  await clickTab("Codex");
  expect(replies.map((r) => r.body)).toEqual([
    { type: "list_agent_models", agent: "claude" },
    { type: "list_agent_models", agent: "codex" },
  ]);
  await act(async () => replies[1].resolve(listing(replies[1].body, [{ id: "cx", name: "Codex model", efforts: ["low"] }])));
  await act(async () => replies[0].resolve(listing(replies[0].body, MODELS)));
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["Codex model"]);
});

it("locks the pickers and says so while the list is out, with the stored value in view", async () => {
  render(stored, [], () => new Promise(() => {}));
  await openAdvanced();
  expect(document.body.textContent).toContain("Loading the models this agent offers");
  expect(locked("Model")).toBe(true);
  expect(locked("Reasoning effort")).toBe(true);
  expect(pickerNamed("Model").textContent).toContain("alpha");
  expect(pickerNamed("Reasoning effort").textContent).toContain("high");
});

it.each([
  ["models_binary_not_found", { binary: "claude" }, "claude was not found, so the models cannot be listed."],
  ["models_shell_environment_failed", { detail: "no shell" }, `The shell environment could not be read, so the models cannot be listed: ${isolate("no shell")}`],
  ["models_timed_out", { agent: "Claude Code" }, `${isolate("Claude Code")} took too long to list its models.`],
  ["models_command_failed", { agent: "Claude Code", detail: "exit 1" }, `${isolate("Claude Code")} could not list its models: ${isolate("exit 1")}`],
  ["models_unreadable_output", { agent: "Claude Code" }, `${isolate("Claude Code")} printed no models that could be read.`],
])("says why there is no list for %s, and leaves the form usable", async (code, params, reason) => {
  const request = render(stored, [], async (body) => listing(body, [], { code, params, message: "daemon words" }));
  await openAdvanced();
  expect(document.body.textContent).toContain(reason);
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["alpha (not in the list)"]);
  await click(pickerNamed("Model"));
  expect(locked("Reasoning effort")).toBe(false);
  await click(clearButton("Reasoning effort"));
  await clickButton("Save");
  expect(sent(request)).toEqual([
    { type: "update_console", console: "c-1", name: "Main", default_agent: "claude", claude_console_session_effort: null },
  ]);
});

it("shows the daemon's own words for a reason the catalogue does not know", async () => {
  render(undefined, [], async (body) => listing(body, [], { code: "models_something_new", params: {}, message: "daemon words" }));
  await openAdvanced();
  expect(document.body.textContent).toContain("daemon words");
});

it("narrows the effort picker to the chosen model's levels", async () => {
  render();
  await openAdvanced();
  await click(pickerNamed("Reasoning effort"));
  // Nothing chosen: every listed level, once, in the order listed.
  expect(optionTexts()).toEqual(["low", "high", "medium", "max"]);
  await click(pickerNamed("Reasoning effort"));
  await click(pickerNamed("Model"));
  await choose("Beta");
  await click(pickerNamed("Reasoning effort"));
  expect(optionTexts()).toEqual(["medium", "high", "max"]);
});

it("leaves the effort picker operable for a model that takes none, and says why there is nothing to pick", async () => {
  render();
  await openAdvanced();
  await click(pickerNamed("Model"));
  await choose("Plain");
  expect(locked("Reasoning effort")).toBe(false);
  expect(document.body.textContent).toContain("This model takes no reasoning effort.");
  await click(pickerNamed("Reasoning effort"));
  expect(optionTexts()).toEqual([]);
});

it("keeps a stored effort clearable once a model that takes none is chosen", async () => {
  const request = render(stored);
  await openAdvanced();
  await click(pickerNamed("Model"));
  await choose("Plain");
  expect(locked("Reasoning effort")).toBe(false);
  await click(clearButton("Reasoning effort"));
  await clickButton("Save");
  expect(sent(request)).toEqual([
    { type: "update_console", console: "c-1", name: "Main", default_agent: "claude", claude_console_session_model: "plain", claude_console_session_effort: null },
  ]);
});

it("names the daemon's own alias for the best model from the catalogue and every other model as sent", async () => {
  render(undefined, [], async (body) => listing(body, [{ id: "best", name: "daemon words", efforts: ["low"] }, { id: "opus", name: "Opus", efforts: ["low"] }]));
  await openAdvanced();
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["Best available", "Opus"]);
});

it("says so when the daemon answers the question with something else", async () => {
  render(undefined, [], async () => ({ type: "ack" }) as Event);
  await openAdvanced();
  expect(document.body.textContent).toContain("The daemon answered with something unexpected.");
});

const codexWork: Account = { id: "a-codex", agent: "codex", name: "Codex work", config_dir: "/home/dev/.codex-work" };
/** The trigger of a picker named by its own label rather than by a visible one. */
const picker = (name: string) => buttons().find((b) => b.getAttribute("aria-label") === name)!;

it("names the agent project sessions follow on the Same as entry, and keeps it current", async () => {
  render(consoleOf("c-1", "Main", { default_agent: "codex" }));
  expect(picker("Project sessions").textContent).toContain("Same as console sessions (Codex)");
  await click(picker("Console sessions"));
  await choose("Grok Build");
  expect(picker("Project sessions").textContent).toContain("Same as console sessions (Grok Build)");
});

it("starts a new console on Same as console sessions and sends no project session agent", async () => {
  const request = render();
  type("Name", "Main");
  await clickButton("Create");
  expect(sent(request)).toEqual([{ type: "create_console", name: "Main", default_agent: "claude" }]);
});

it("creates with the project session agent that was picked", async () => {
  const request = render();
  type("Name", "Main");
  await click(picker("Project sessions"));
  await choose("Codex");
  await clickButton("Create");
  expect(sent(request)).toEqual([{ type: "create_console", name: "Main", default_agent: "claude", project_session_agent: "codex" }]);
});

it("opens a console stored without a project session agent on Same as and sends nothing for it", async () => {
  const request = render(consoleOf("c-1", "Main", { default_agent: "codex" }));
  expect(picker("Project sessions").textContent).toContain("Same as console sessions");
  type("Name", "Renamed");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Renamed", default_agent: "codex" }]);
});

it("sends the project session agent only when it changed, and an explicit null when it goes back to Same as", async () => {
  const request = render(consoleOf("c-1", "Main", { project_session_agent: "grok" }));
  expect(picker("Project sessions").textContent).toContain("Grok Build");
  await clickButton("Save");
  expect(sent(request)[0]).toEqual({ type: "update_console", console: "c-1", name: "Main", default_agent: "claude" });
  await click(picker("Project sessions"));
  await choose("Same as console sessions (Claude Code)");
  await clickButton("Save");
  expect(sent(request)[1]).toEqual({ type: "update_console", console: "c-1", name: "Main", default_agent: "claude", project_session_agent: null });
});

it("lets the console session agent change without moving the project session agent", async () => {
  const request = render(consoleOf("c-1", "Main", { project_session_agent: "grok" }));
  await click(picker("Console sessions"));
  await choose("Codex");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "codex" }]);
});

it("follows the console session agent in the Advanced section, not the project session agent", async () => {
  const request = render(consoleOf("c-1", "Main", { default_agent: "codex", project_session_agent: "grok" }));
  await openAdvanced();
  expect(asked(request)).toEqual([{ type: "list_agent_models", agent: "codex" }]);
});

it("shows one row per agent in the accounts table, each with its own account", async () => {
  render(consoleOf("c-1", "Main", { codex_account_id: "a-codex" }), [work, codexWork]);
  expect(picker("Claude Code account").textContent).toContain("Default");
  expect(picker("Codex account").textContent).toContain("Codex work");
  // Grok has only its default account: stated, with the way to add another.
  expect(buttons().filter((b) => b.textContent === "Manage accounts…")).toHaveLength(1);
  expect(document.body.textContent).toContain("Default account");
});

it("sends an account chosen for an agent that is not the console session agent", async () => {
  const request = render(consoleOf("c-1", "Main"), [work, codexWork]);
  await click(picker("Codex account"));
  await choose("Codex work");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "claude", codex_account_id: "a-codex" }]);
});

it("opens Settings at the agent accounts from an agent that has only its default account", async () => {
  // The Settings dialog sizes itself by a breakpoint the stylesheet defines.
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: true, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  render();
  await clickButton("Manage accounts…");
  const settings = document.body.querySelector('[aria-label="Settings"]');
  expect(settings).not.toBeNull();
  expect(settings?.querySelector('[role="tab"][aria-selected="true"]')?.textContent).toBe("Agent accounts");
});

it("keeps the console dialog open and unsaved while an account is added in the Settings over it", async () => {
  vi.stubGlobal("matchMedia", (media: string) => ({ media, matches: true, addEventListener() {}, removeEventListener() {}, addListener() {}, removeListener() {} }));
  document.documentElement.style.setProperty("--breakpoint-docked", "1168px");
  onTestFinished(() => {
    vi.unstubAllGlobals();
    document.documentElement.style.removeProperty("--breakpoint-docked");
  });
  const request = render(consoleOf("c-1", "Main"));
  await clickButton("Manage accounts…");
  await clickButton("Add account");
  const fill = (label: string, value: string) => {
    const input = Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).filter((el) => el.labels?.[0]?.textContent === label).at(-1)!;
    act(() => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
  };
  fill("Name", "Work");
  fill("Config directory", "/home/dev/.claude-work");
  await clickButton("Add");
  expect(sent(request)).toEqual([{ type: "create_account", agent: "claude", name: "Work", config_dir: "/home/dev/.claude-work" }]);
  expect(document.body.textContent).toContain("Console settings");
});

it("lists only the agents that may be picked, and the accounts of those", () => {
  render(undefined, [work, codexWork], undefined, { codex: "unavailable" });
  expect(picker("Console sessions")).toBeDefined();
  expect(picker("Claude Code account")).toBeDefined();
  expect(buttons().some((b) => b.getAttribute("aria-label") === "Codex account")).toBe(false);
  expect(document.body.textContent).not.toContain("Codex (not installed)");
});

it("leaves the missing agents out of both default agent pickers", async () => {
  render(undefined, [], undefined, { codex: "unavailable" });
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code", "Grok Build"]);
  await choose("Claude Code");
  await click(picker("Project sessions"));
  expect(optionTexts()).toEqual(["Same as console sessions (Claude Code)", "Claude Code", "Grok Build"]);
});

it("keeps the stored agent in its picker as not installed while it is the value, and drops it once moved off", async () => {
  render(consoleOf("c-1", "Main", { default_agent: "codex" }), [], undefined, { codex: "unavailable" });
  expect(picker("Console sessions").textContent).toContain("Codex (not installed)");
  await click(picker("Console sessions"));
  const codex = Array.from(document.body.querySelectorAll('[role="option"]')).find((o) => o.textContent === "Codex (not installed)");
  expect(codex?.getAttribute("aria-selected")).toBe("true");
  await choose("Claude Code");
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code", "Grok Build"]);
});

it("keeps a missing agent's row while the console pins a named account for it, and drops it once moved off", async () => {
  render(consoleOf("c-1", "Main", { codex_account_id: "a-codex" }), [work, codexWork], undefined, { codex: "unavailable" });
  expect(picker("Codex account").textContent).toContain("Codex work");
  expect(document.body.textContent).toContain("Codex (not installed)");
  await click(picker("Codex account"));
  await choose("Default");
  expect(document.body.textContent).not.toContain("Codex (not installed)");
});

it("lists every agent before availability has been determined", async () => {
  render();
  await click(picker("Console sessions"));
  expect(optionTexts()).toEqual(["Claude Code", "Codex", "Grok Build"]);
});

const tabNames = () => Array.from(document.body.querySelectorAll('[role="tab"]')).map((tab) => tab.textContent);
const selectedTab = () => document.body.querySelector('[role="tab"][aria-selected="true"]')?.textContent;

it("opens the Advanced section on the console session agent, with a tab for each installed agent", async () => {
  render(consoleOf("c-1", "Main", { default_agent: "codex" }));
  await openAdvanced();
  expect(tabNames()).toEqual(["Claude Code", "Codex", "Grok Build"]);
  expect(selectedTab()).toBe("Codex");
});

it("holds the console session and project session values apart for the agent shown", async () => {
  render(consoleOf("c-1", "Main", { claude_console_session_model: "alpha", claude_project_session_model: "beta" }));
  await openAdvanced();
  expect(groupPicker("Console sessions", "Model").textContent).toContain("Alpha");
  expect(groupPicker("Project sessions", "Model").textContent).toContain("Beta");
});

it("keeps the edits of every agent while switching tabs, and saves only what changed", async () => {
  const request = render(consoleOf("c-1", "Main", { codex_project_session_model: "kept" }));
  await openAdvanced();
  await click(groupPicker("Project sessions", "Model"));
  await choose("Alpha");
  await clickTab("Codex");
  expect(groupPicker("Project sessions", "Model").textContent).toContain("kept");
  await click(groupPicker("Console sessions", "Reasoning effort"));
  await choose("high");
  await clickTab("Claude Code");
  expect(groupPicker("Project sessions", "Model").textContent).toContain("Alpha");
  await clickButton("Save");
  expect(sent(request)).toEqual([
    {
      type: "update_console",
      console: "c-1",
      name: "Main",
      default_agent: "claude",
      claude_project_session_model: "alpha",
      codex_console_session_effort: "high",
    },
  ]);
});

it("creates with the project session pair that was set", async () => {
  const request = render();
  type("Name", "Main");
  await openAdvanced();
  await click(groupPicker("Project sessions", "Model"));
  await choose("Beta");
  await click(groupPicker("Project sessions", "Reasoning effort"));
  await choose("max");
  await clickButton("Create");
  expect(sent(request)).toEqual([
    { type: "create_console", name: "Main", default_agent: "claude", claude_project_session_model: "beta", claude_project_session_effort: "max" },
  ]);
});

it("sends an explicit null when a project session value is cleared", async () => {
  const request = render(consoleOf("c-1", "Main", { claude_project_session_model: "alpha" }));
  await openAdvanced();
  await click(groupPicker("Project sessions", "Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]'));
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Main", default_agent: "claude", claude_project_session_model: null }]);
});

it("leaves a missing agent without a tab unless the console stores a value for it, and drops it once cleared", async () => {
  render(consoleOf("c-1", "Main", { codex_project_session_model: "kept" }), [], undefined, { codex: "unavailable", grok: "unavailable" });
  await openAdvanced();
  expect(tabNames()).toEqual(["Claude Code", "Codex (not installed)"]);
  await clickTab("Codex (not installed)");
  await click(groupPicker("Project sessions", "Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]'));
  expect(tabNames()).toEqual(["Claude Code"]);
  expect(selectedTab()).toBe("Claude Code");
});

it("follows the console session agent in the Advanced section until a tab is picked", async () => {
  render();
  await click(picker("Console sessions"));
  await choose("Codex");
  await openAdvanced();
  expect(selectedTab()).toBe("Codex");
  await click(picker("Console sessions"));
  await choose("Grok Build");
  expect(selectedTab()).toBe("Grok Build");
  await clickTab("Claude Code");
  await click(picker("Console sessions"));
  await choose("Codex");
  expect(selectedTab()).toBe("Claude Code");
});

it("does not take the view back to an agent picked earlier that comes back", async () => {
  render(consoleOf("c-1", "Main", { codex_project_session_model: "kept" }), [], undefined, { codex: "unavailable" });
  await openAdvanced();
  await clickTab("Codex (not installed)");
  await click(groupPicker("Project sessions", "Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]'));
  expect(selectedTab()).toBe("Claude Code");
  await act(async () => lastStore.setState({ agentAvailability: new Map([["claude", { agent: "claude", availability: "available" }], ["codex", { agent: "codex", availability: "available" }], ["grok", { agent: "grok", availability: "available" }]]) }));
  expect(tabNames()).toEqual(["Claude Code", "Codex", "Grok Build"]);
  expect(selectedTab()).toBe("Claude Code");
});

it("names each Advanced picker by its sessions and its setting, and describes it by their note and the model list's state", async () => {
  render(undefined, [], async (body) => listing(body, [], { code: "models_timed_out", params: { agent: "Claude Code" }, message: "" }));
  await openAdvanced();
  const pickers = [["Console sessions", "Model"], ["Console sessions", "Reasoning effort"], ["Project sessions", "Model"], ["Project sessions", "Reasoning effort"]];
  const names = pickers.map(([group, name]) => nameOf(groupPicker(group, name)).join(" "));
  expect(names).toEqual(["Console sessions Model", "Console sessions Reasoning effort", "Project sessions Model", "Project sessions Reasoning effort"]);
  expect(new Set(names).size).toBe(4);
  // Named only by the headers: an own aria-label beside them would be read as part of the name.
  for (const [group, name] of pickers) expect(groupPicker(group, name).getAttribute("aria-label")).toBeNull();
  const describedBy = (group: string, name: string) =>
    (groupPicker(group, name).getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "");
  expect(describedBy("Console sessions", "Reasoning effort")).toEqual([expect.stringContaining("Console sessions opened or resumed from now on")]);
  expect(describedBy("Project sessions", "Reasoning effort")).toEqual([expect.stringContaining("Project sessions opened or resumed from now on")]);
  // The model list's state stands under both Model pickers, besides the note on their sessions.
  const notes = Array.from(document.body.querySelectorAll('[data-slot="description"]')).filter((el) => el.textContent?.includes("took too long to list its models"));
  expect(notes).toHaveLength(2);
  expect(groupPicker("Console sessions", "Model").getAttribute("aria-describedby")).toContain(notes[0].id);
  expect(groupPicker("Project sessions", "Model").getAttribute("aria-describedby")).toContain(notes[1].id);
  expect(describedBy("Console sessions", "Model")).toHaveLength(2);
  // The headers that name the pickers are hidden from assistive technology, so they are not read a
  // second time beside the names, and no visible text of the table is outside them.
  for (const [group, name] of pickers) {
    for (const id of groupPicker(group, name).getAttribute("aria-labelledby")!.split(" ")) {
      const el = document.getElementById(id)!;
      if (el.textContent === group || el.textContent === name) expect(el.closest('[aria-hidden="true"]')).not.toBeNull();
    }
  }
  const headers = Array.from(document.body.querySelectorAll("div, span")).filter((el) => ["Console sessions", "Project sessions", "Reasoning effort"].includes(el.textContent ?? "") && el.children.length === 0 && !el.closest("button, label, [role=tab]"));
  expect(headers.length).toBeGreaterThan(0);
  expect(headers.every((el) => el.closest('[aria-hidden="true"]'))).toBe(true);
});
