// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { consoleOf } from "../gallery/fixtures/builders";
import { isolate } from "../i18n/isolate";
import type { Event, RequestBody } from "../protocol";
import { openDetails } from "../components/detailsTestSupport";
import { work, personal, MODELS, listing, render, click, clickButton, choose, sent, asked, openAdvanced, clickTab, pickerNamed, clearButton, stored, optionTexts } from "./consoleDialogTestSupport";

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
  expect(pickerNamed("Model").textContent).toContain("retired, Not in the list");
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["Alpha", "Beta", "Plain", "retired, Not in the list"]);
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

/** The status regions assistive technology is given, read once their first write delay has passed, so what
 * they say is part of what is checked. */
async function exposedStatusesAfterFirstWrite(): Promise<Element[]> {
  await act(async () => new Promise((resolve) => setTimeout(resolve, 150)));
  return Array.from(document.body.querySelectorAll('[role="status"]')).filter((node) => !node.closest("[hidden], [inert], [aria-hidden='true']"));
}

it.each([
  ["models_binary_not_found", { binary: "claude" }, "claude was not found, so the models cannot be listed.", `${isolate("claude")} was not found, so the models cannot be listed.`],
  ["models_shell_environment_failed", { detail: "no shell" }, "The shell environment could not be read, so the models cannot be listed.", "The shell environment could not be read, so the models cannot be listed."],
  ["models_timed_out", { agent: "Claude Code" }, "Claude Code took too long to list its models.", `${isolate("Claude Code")} took too long to list its models.`],
  ["models_command_failed", { agent: "Claude Code", detail: "exit 1" }, "Claude Code could not list its models.", `${isolate("Claude Code")} could not list its models.`],
  ["models_unreadable_output", { agent: "Claude Code" }, "Claude Code printed no models that could be read.", `${isolate("Claude Code")} printed no models that could be read.`],
])("says why there is no list for %s, and leaves the form usable", async (code, params, summary, reason) => {
  const request = render(stored, [], async (body) => listing(body, [], { code, params, message: "daemon words" }));
  await openAdvanced();
  const descriptions = Array.from(document.body.querySelectorAll('[data-slot="alert-description"]')).filter((node) => node.textContent === summary);
  // One Alert under the table, however many Model pickers it describes.
  expect(descriptions).toHaveLength(1);
  for (const description of descriptions) {
    expect(description.textContent).not.toMatch(/[\u2066-\u2069]/);
    expect(description.querySelector("pre")).toBeNull();
    const reference = description.querySelector("[data-inline-reference]");
    if ("binary" in params) {
      expect(reference?.textContent).toBe(params.binary);
      expect(reference?.getAttribute("dir")).toBe("ltr");
      expect(reference?.querySelector("code")?.textContent).toBe(params.binary);
    } else if ("agent" in params) {
      expect(reference?.textContent).toBe(params.agent);
      expect(reference?.getAttribute("dir")).toBe("auto");
      expect(reference?.querySelector("code")).toBeNull();
    } else expect(reference).toBeNull();
    if (reference) expect(reference.getAttribute("data-size")).toBe("xs");
  }
  expect(document.body.querySelector('[data-slot="alert-root"] pre')).toBeNull();
  const statuses = await exposedStatusesAfterFirstWrite();
  // What the region says is the Alert's sentence alone: neither the remedy nor the detail.
  expect(statuses.map((node) => node.textContent)).toEqual([reason]);
  expect(statuses[0].querySelector("[data-inline-reference], code, pre")).toBeNull();
  // The diagnostics are behind the Alert's Details button: in its popover once opened, which is closed again
  // before the rest, since an open popover hides what is behind it.
  const detail = "detail" in params ? params.detail : undefined;
  if (detail) {
    const alert = document.body.querySelector<HTMLElement>('[data-slot="alert-root"]')!;
    const dialog = openDetails(alert);
    expect(Array.from(dialog.querySelectorAll("pre")).map((node) => node.textContent)).toEqual([detail]);
    act(() => void dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  }
  await click(pickerNamed("Model"));
  expect(optionTexts()).toEqual(["alpha, Not in the list"]);
  await click(pickerNamed("Model"));
  expect(locked("Reasoning effort")).toBe(false);
  await click(clearButton("Reasoning effort"));
  await clickButton("Save");
  expect(sent(request)).toEqual([
    { type: "update_console", console: "c-1", name: "Main", default_agent: "claude", claude_console_session_effort: null },
  ]);
});

it("shows the generic sentence for a reason the catalogue does not know, with the daemon's own words behind Details", async () => {
  render(undefined, [], async (body) => listing(body, [], { code: "models_something_new", params: {}, message: "daemon words" }));
  await openAdvanced();
  const statuses = await exposedStatusesAfterFirstWrite();
  expect(statuses.map((node) => node.textContent)).toEqual(["Something unexpected went wrong."]);
  expect(document.body.textContent).not.toContain("daemon words");
  expect(openDetails(document.body.querySelector('[data-slot="alert-root"]')!).textContent).toBe("daemon words");
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
