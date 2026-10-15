// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { Account, AgentModel, Console, Event, ModelsUnavailable, RequestBody } from "../protocol";
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

/** Renders the dialog over a daemon that answers every request, with `MODELS` unless `respond` says
 * otherwise, and returns the requests sent. */
function render(editing?: Console, accounts: Account[] = [], respond?: (body: RequestBody) => Promise<Event>) {
  const request = vi.fn((body: RequestBody) => respond?.(body) ?? Promise.resolve(body.type === "list_agent_models" ? listing(body, MODELS) : ({ type: "ack" } as Event)));
  const daemon: Daemon = {
    store: createStateStore({ settings: { auto_pull_repositories: false, default_clone_dir: "", accounts } }),
    request,
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
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
/** What the form sent apart from its questions about models. */
const sent = (request: ReturnType<typeof render>) => request.mock.calls.map((call) => call[0]).filter((body) => body.type !== "list_agent_models");
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
const pickerNamed = (name: string) =>
  buttons().find((b) => b.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.textContent === name))!;
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
  expect(document.body.textContent).toContain("Used by console sessions");
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

it("says so at the foot of the dialog when the picked file cannot be read, and keeps the avatar", async () => {
  vi.mocked(imageToAvatar).mockRejectedValue(new Error("not an image"));
  render(withAvatar);
  await pickFile();
  expect(document.body.querySelector('[role="alert"]')?.textContent).toContain("could not be read as an image");
  expect(document.body.querySelector("img")?.getAttribute("src")).toBe(AVATAR);
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
  await clickButton((b) => b.textContent?.includes("Claude Code") ?? false);
  await choose("Codex");
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
  await clickButton((b) => b.textContent?.includes("Claude Code") ?? false);
  await choose("Codex");
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
  ["models_binary_not_found", { binary: "claude" }, `${isolate("claude")} was not found, so the models cannot be listed.`],
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
