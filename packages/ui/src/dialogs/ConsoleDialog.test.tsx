// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { errorAlerts } from "../errorAlerts";
import { consoleOf } from "../gallery/fixtures/builders";
import { imageToAvatar } from "../avatarImage";
import { work, personal, render, buttons, click, clickButton, choose, sent, type, openAdvanced, pickerNamed, clearButton, stored } from "./consoleDialogTestSupport";

// Decoding an image needs a canvas, which jsdom does not have; each test says what the read gives.
vi.mock("../avatarImage", () => ({ imageToAvatar: vi.fn() }));

const PLACEHOLDER = "The agent's own setting";

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
  expect(document.body.textContent).toContain("For project sessions, settings on the project or session take precedence.");
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
