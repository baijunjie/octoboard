// @vitest-environment jsdom
import { act } from "react";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it } from "vitest";

import { DaemonRequestError } from "../daemon-client";
import { projectOf } from "../gallery/fixtures/builders";
import type { Agent } from "../protocol";
import { roots, work, personal, render, input, type, settle, click, answer, chooseSource, buttonText, accountPicker, pickerNamed, sentOf, fillDirectory, stored } from "./projectDialogTestSupport";

/** The hidden native select behind the Default agent field, which holds the chosen value. */
const agentValue = () =>
  Array.from(document.body.querySelectorAll("select")).find((el) => Array.from(el.options).some((o) => o.value === "codex"))!.value;

it("opens on the directory when adding and on the name when editing, so Enter submits at once", () => {
  act(() => setInteractionModality("pointer"));
  render();
  expect(document.activeElement).toBe(input("Directory"));
  act(() => roots.pop()!.unmount());
  document.body.replaceChildren();
  render(stored);
  expect(document.activeElement).toBe(input("Name"));
});

it("preselects the agent detected for a typed directory, and only the latest answer counts", async () => {
  const pending = render();
  type(input("Directory"), "~/code/a");
  await settle(500);
  type(input("Directory"), "~/code/b");
  await settle(500);
  expect(pending.map((p) => p.body)).toEqual([
    { type: "detect_directory_agent", path: "~/code/a" },
    { type: "detect_directory_agent", path: "~/code/b" },
  ]);
  await answer(pending[1], "grok");
  await answer(pending[0], "codex");
  expect(agentValue()).toBe("grok");
});

it.each(["path_not_found", "path_not_a_directory"])("shows %s as the Directory field's own error, with the path as danger-toned code", async (code) => {
  const pending = render();
  type(input("Directory"), "~/code/missing");
  await settle(500);
  await act(async () => pending[0].reject(new DaemonRequestError("plain", code, { path: "/Users/dev/code/missing" })));
  const error = document.body.querySelector('[data-slot="field-error"] [role="alert"]');
  expect(error?.querySelector("code")?.textContent).toBe("/Users/dev/code/missing");
  expect(error?.querySelector("code")?.getAttribute("data-color")).toBe("danger");
  expect(input("Directory").getAttribute("aria-invalid")).toBe("true");
  expect(document.body.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(input("Name").disabled).toBe(true);
});

it("keeps an Alert under the Directory field when the check could not run", async () => {
  const pending = render();
  type(input("Directory"), "~/code/a");
  await settle(500);
  await act(async () => pending[0].reject(new DaemonRequestError("plain", "directory_unreachable", { path: "/Users/dev/code/a" })));
  const alert = document.body.querySelector('[data-slot="alert-root"]')!.parentElement!;
  expect(input("Directory").getAttribute("aria-describedby")?.split(" ")).toContain(alert.id);
  expect(document.body.querySelector('[data-slot="field-error"] [role="alert"]')?.textContent ?? "").toBe("");
  expect(input("Name").disabled).toBe(true);
});

it("keeps the form locked while the answer for the path as it stands is out", async () => {
  const pending = render();
  type(input("Directory"), "~/code/a");
  // Locked from the keystroke on: the wait before the ask is part of the check.
  expect(input("Name").disabled).toBe(true);
  await settle(500);
  expect(input("Name").disabled).toBe(true);
  await answer(pending[0], null);
  expect(input("Name").disabled).toBe(false);
});

it("keeps the git form locked until the probe succeeds", async () => {
  const pending = render();
  await chooseSource("git repository URL");
  const clone = () => input("Clone into (parent directory)");
  expect(clone().disabled).toBe(true);
  type(input("Repository URL"), "https://example.com/o/r.git");
  await settle(900);
  expect(pending[0].body).toEqual({ type: "probe_git_remote", remote_url: "https://example.com/o/r.git" });
  expect(clone().disabled).toBe(true);
  await answer(pending[0], null);
  expect(clone().disabled).toBe(false);
});

it.each([
  { name: "a detected agent is stored as shown", source: "A single directory", detected: "codex" as Agent | null, sent: { default_agent: "codex", detect_default_agent: false } },
  { name: "no detected agent stays inherited", source: "A single directory", detected: null, sent: { detect_default_agent: false } },
  { name: "a parent directory detects per repository", source: "A parent directory", detected: null, sent: {} },
])("sends add_project with $name", async ({ source, detected, sent }) => {
  const pending = render();
  if (source !== "A single directory") await chooseSource(source);
  type(input("Directory"), "~/code/x");
  if (source === "A single directory") {
    await settle(500);
    await answer(pending[0], detected);
  }
  await click(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent === "Add"));
  const added = pending.find((p) => p.body.type === "add_project")?.body;
  expect(added).toMatchObject({ type: "add_project", source: source === "A single directory" ? "local" : "parent", ...sent });
  if (!("default_agent" in sent)) expect(added).not.toHaveProperty("default_agent", expect.anything());
});

const typeDescription = (value: string) => type(document.body.querySelector<HTMLTextAreaElement>("textarea")!, value);

it("adds with the description that was written, trimmed", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  typeDescription("  The marketing website.  ");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ description: "The marketing website." });
});

it("adds without a description when it is left blank", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  typeDescription("   ");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).not.toHaveProperty("description");
});

it("hides the name and sends none while the source is a parent directory", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  type(input("Name"), "Site");
  await chooseSource("A parent directory");
  expect(input("Name")).toBeUndefined();
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ name: undefined });
});

it("hides the description and sends none while the source is a parent directory", async () => {
  const pending = render(undefined, [work, personal]);
  await fillDirectory(pending);
  typeDescription("The marketing website.");
  await chooseSource("A parent directory");
  expect(document.body.querySelector("textarea")).toBeNull();
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).not.toHaveProperty("description");
});

const described = projectOf("p-4", "c-1", "Site", { description: "The marketing website." });

it("sends nothing about the description when editing leaves it untouched", async () => {
  const pending = render(described);
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).not.toHaveProperty("description");
});

it("sends the description when editing changes it", async () => {
  const pending = render(described);
  typeDescription(" The public website. ");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toMatchObject({ description: "The public website." });
});

it("sends an explicit null when editing clears the description", async () => {
  const pending = render(described);
  typeDescription("  ");
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toMatchObject({ description: null });
});

it.each([
  { label: "Name", limit: 100, sample: "😀", message: "Name must be 100 characters or fewer." },
  { label: "Description", limit: 2000, sample: "中", message: "Description must be 2,000 characters or fewer." },
])("counts $label as trimmed Unicode characters, keeps excess text, and blocks saving until corrected", async ({ label, limit, sample, message }) => {
  const pending = render();
  await fillDirectory(pending);
  const field = label === "Name" ? input(label) : document.querySelector<HTMLTextAreaElement>("textarea")!;
  type(field, `  ${sample.repeat(limit + 1)}  `);
  expect(document.body.textContent).toContain(`${(limit + 1).toLocaleString("en")} / ${limit.toLocaleString("en")} characters`);
  await click(buttonText("Add"));
  expect(pending.some((p) => p.body.type === "add_project")).toBe(false);
  expect(document.body.textContent).toContain(message);
  expect(field.value.trim()).toBe(sample.repeat(limit + 1));
  expect(field.getAttribute("aria-invalid")).toBe("true");
  expect(document.querySelector('[role="alert"]')?.textContent).toContain(`${(limit + 1).toLocaleString("en")} / ${limit.toLocaleString("en")} characters`);
  const errorId = field.getAttribute("aria-describedby")?.split(" ").find((id) => document.getElementById(id)?.textContent?.includes(message));
  expect(errorId).toBeDefined();
  type(field, sample.repeat(limit));
  expect(document.body.textContent).not.toContain(message);
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ [label.toLowerCase()]: sample.repeat(limit) });
});

it("keeps legacy overlong fields while saving other settings, but rejects changed overlong text", async () => {
  const legacy = projectOf("p-long", "c-1", "中".repeat(101), { description: "😀".repeat(2001) });
  const pending = render(legacy);
  await click(buttonText("Save"));
  expect(sentOf(pending, "update_project")).toMatchObject({ name: legacy.name });
  expect(sentOf(pending, "update_project")).not.toHaveProperty("description");
  await act(async () => pending.find((p) => p.body.type === "update_project")!.resolve({ type: "project_upserted", project: legacy }));
  typeDescription(`${legacy.description}a`);
  await click(buttonText("Save"));
  expect(pending.filter((p) => p.body.type === "update_project")).toHaveLength(1);
  expect(document.body.textContent).toContain("Description must be 2,000 characters or fewer.");
});

it("shows the name counter only near the limit and leaves hidden parent fields out of validation", async () => {
  const pending = render();
  await fillDirectory(pending);
  type(input("Name"), "a".repeat(89));
  expect(document.body.textContent).not.toContain("/ 100 characters");
  type(input("Name"), "a".repeat(90));
  expect(document.body.textContent).toContain("90 / 100 characters");
  type(input("Name"), "a".repeat(101));
  typeDescription("a".repeat(2001));
  await chooseSource("A parent directory");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ source: "parent" });
});

it.each(["trigger", "popover"] as const)("hands focus back to the agent picker when the last named account is removed from %s", async (focused) => {
  const { store } = render(projectOf("p-2", "c-1", "Site", { default_agent: "claude" }), [work]);
  const agentTrigger = pickerNamed("Default agent");
  act(() => accountPicker()!.focus());
  if (focused === "popover") {
    await click(accountPicker());
    await settle(50);
    expect(document.activeElement?.getAttribute("role")).toBe("option");
  }
  act(() => store.setState({ settings: { ...store.getState().settings!, accounts: [] } }));
  expect(accountPicker()).toBeUndefined();
  await settle(50);
  expect(document.activeElement).toBe(agentTrigger);
});

it("updates the optional name hint with the normalized directory without changing the empty submitted name", async () => {
  const pending = render();
  act(() => pending.store.setState({ homeDir: "/home/dev" }));
  expect(input("Name").placeholder).toBe("Directory name");
  type(input("Directory"), "~/site/../website/./");
  expect(input("Name").placeholder).toBe("website");
  await settle(500);
  await answer(pending[0], null);
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ path: "~/site/../website/./", name: undefined, tags: [] });
});

it("uses the repository's clone name as a live hint, preserving an entered name and empty clone submission", async () => {
  const pending = render();
  type(input("Name"), "My name");
  await chooseSource("git repository URL");
  type(input("Repository URL"), "git@example.com:owner/site.git");
  expect(input("Name").placeholder).toBe("site");
  expect(input("Name").value).toBe("My name");
  type(input("Repository URL"), "https://example.com/owner/other.git/");
  expect(input("Name").placeholder).toBe("other");
  expect(input("Name").value).toBe("My name");
  await settle(900);
  await answer(pending[0], null);
  type(input("Name"), "");
  await click(buttonText("Add"));
  expect(sentOf(pending, "add_project")).toMatchObject({ source: "git", path: undefined, name: undefined, remote_url: "https://example.com/owner/other.git/", tags: [] });
});

it("keeps an example on the populated and cleared required edited project name", () => {
  render(stored);
  expect(input("Name").placeholder).toBe("Ex. Website");
  type(input("Name"), "");
  expect(input("Name").placeholder).toBe("Ex. Website");
});

// The directory browser opens over this dialog, whose focus containment stays on: a path field focused before the
// browser's focus scope is set up reads to it as focus escaping, is pulled back to Browse, and the browser's dialog then
// takes focus itself. A press without a pointer leaves the move back to a frame later, by when this dialog is inert.
it.each(["keyboard", "mouse"] as const)("opens the directory browser on its path field after a %s press, and Escape closes only the browser", async (device) => {
  // As the dialog was opened, by the same device, so its own opening focus is not put off a frame.
  act(() => setInteractionModality(device === "keyboard" ? "keyboard" : "pointer"));
  render();
  const browse = buttonText("Browse")!;
  act(() => browse.focus());
  act(() => {
    if (device === "keyboard") {
      browse.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
      browse.dispatchEvent(new KeyboardEvent("keyup", { key: "Enter", bubbles: true, cancelable: true }));
    } else {
      // A pointer event of no size reads as a virtual one to react-aria.
      const pointer = { bubbles: true, cancelable: true, pointerType: "mouse", pointerId: 1, isPrimary: true, button: 0, width: 1, height: 1, detail: 1 };
      browse.dispatchEvent(new PointerEvent("pointerdown", pointer));
      browse.dispatchEvent(new PointerEvent("pointerup", pointer));
      browse.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, button: 0, detail: 1 }));
    }
  });
  expect(getInteractionModality()).toBe(device === "keyboard" ? "keyboard" : "pointer");
  const path = document.body.querySelector<HTMLInputElement>('input[aria-label="Directory path"]');
  expect(path).not.toBeNull();
  expect(document.activeElement).toBe(path);
  act(() => void path!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true })));
  await settle(50);
  expect(document.body.querySelector('input[aria-label="Directory path"]')).toBeNull();
  expect(input("Directory")).toBeDefined();
  expect(document.activeElement).toBe(browse);
});
