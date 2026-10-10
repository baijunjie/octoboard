// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { Agent, Event, RequestBody } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ProjectDialog } from "./ProjectDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** A daemon whose replies the test hands out itself, in whatever order it likes. */
type Pending = { body: RequestBody; resolve: (event: Event) => void };
const roots: Root[] = [];

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  for (const root of roots.splice(0)) act(() => root.unmount());
  document.body.replaceChildren();
  vi.useRealTimers();
});

function render() {
  const pending: Pending[] = [];
  const daemon: Daemon = {
    store: createStateStore({}),
    request: (body) => new Promise((resolve) => pending.push({ body, resolve })),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  roots.push(root);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <ProjectDialog consoleId="c-1" onClose={() => {}} />
      </DaemonProvider>,
    ),
  );
  return pending;
}

const input = (label: string) =>
  Array.from(document.body.querySelectorAll<HTMLInputElement>("input")).find((el) => el.labels?.[0]?.textContent === label)!;

/** The hidden native select behind the Default agent field, which holds the chosen value. */
const agentValue = () =>
  Array.from(document.body.querySelectorAll("select")).find((el) => Array.from(el.options).some((o) => o.value === "codex"))!.value;

function type(el: HTMLInputElement, value: string) {
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

const settle = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
const click = (el: HTMLElement | undefined) => act(async () => el?.click());
const answer = (request: Pending | undefined, agent: Agent | null) =>
  act(async () => request?.resolve({ type: "agent_detected", agent }));

async function chooseSource(label: string) {
  await click(Array.from(document.body.querySelectorAll("button")).find((b) => b.textContent?.includes("A single directory")));
  await click(Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]')).find((o) => o.textContent?.includes(label)));
}

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

it("keeps the form locked while the answer for the path as it stands is out", async () => {
  const pending = render();
  type(input("Directory"), "~/code/a");
  // Locked from the keystroke on: the wait before the ask is part of the check.
  expect(input("Name (optional)").disabled).toBe(true);
  await settle(500);
  expect(input("Name (optional)").disabled).toBe(true);
  await answer(pending[0], null);
  expect(input("Name (optional)").disabled).toBe(false);
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
