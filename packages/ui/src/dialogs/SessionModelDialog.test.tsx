// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import type { Session } from "../protocol";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { framesAsTimers, trackMountRefocus } from "./dialogTestSupport";
import { SessionModelDialog } from "./SessionModelDialog";

vi.mock("../theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const parentConsole = consoleOf("c-1", "Main");
const project = { ...projectOf("p-1", parentConsole.id, "Search API"), claude_session_model: "opus", claude_session_effort: "high" };

async function mount(session: Session, models: "pending" | "unavailable" | "empty" | "listed" = "listed") {
  const store = createStateStore({
    consoles: new Map([[parentConsole.id, parentConsole]]),
    projects: new Map([[project.id, project]]),
    sessions: new Map([[session.id, session]]),
  });
  const request = vi.fn(async (body: { type: string; agent?: string; account?: string | null }) => {
    if (body.type !== "list_agent_models") return { type: "ack" };
    if (models === "pending") return new Promise<never>(() => {});
    return { type: "agent_models", agent: body.agent, account: body.account ?? null, ...(models === "unavailable"
      ? { models: [], unavailable: { code: "models_binary_not_found", params: { binary: "claude" }, message: "claude was not found" } }
      : models === "empty" ? { models: [], unavailable: null }
        : { models: [
          { id: "opus", name: "Opus", efforts: ["low", "high"] },
          { id: "sonnet", name: "Sonnet", efforts: ["low", "high"] },
        ], unavailable: null }) };
  });
  const daemon = { store, request, toastError: vi.fn(), toastNotice: vi.fn(), onToast: () => () => {}, dismissTrustPrompt: () => {}, reconnect: () => {}, terminalUrl: () => "" } as unknown as Daemon;
  const onClose = vi.fn();
  const onChangeModel = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() => root.render(<DaemonProvider value={daemon}><SessionModelDialog session={session} onClose={onClose} onChangeModel={onChangeModel} /></DaemonProvider>));
  const pastMountRefocus = trackMountRefocus();
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  await pastMountRefocus();
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 0)); });
  return { request, onClose, onChangeModel, store };
}

const buttons = () => Array.from(document.body.querySelectorAll<HTMLButtonElement>("button"));
const confirm = () => buttons().find((button) => button.textContent === "Change model")!;
const options = () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]'));
const picker = (name: string) => buttons().find((button) =>
  button.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.textContent === name))!;

it("opens on Cancel while confirming is disabled until a value changes", async () => {
  framesAsTimers("pointer");
  await mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle"));
  const cancel = buttons().find((button) => button.textContent === "Cancel")!;
  expect(document.activeElement).toBe(cancel);
  expect(cancel.hasAttribute("data-opening-focus")).toBe(true);
  expect(confirm().disabled).toBe(true);
});

it.each(["mark", "popover"] as const)("hands focus from the model-failure %s to Cancel when an account change removes it and locks both pickers", async (from) => {
  framesAsTimers("pointer");
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle");
  const { request, store } = await mount(session, "unavailable");
  const mark = document.querySelector<HTMLButtonElement>("button[data-status-mark]")!;
  if (from === "popover") {
    await act(async () => mark.click());
    expect(document.querySelector("[data-status-mark-popover]")!.contains(document.activeElement)).toBe(true);
  } else act(() => mark.focus());
  request.mockImplementation(async () => new Promise<never>(() => {}));
  await act(async () => store.setState({ sessions: new Map([[session.id, { ...session, account_id: "other" }]]) }));
  expect(document.querySelector("button[data-status-mark]")).toBeNull();
  expect(picker("Model").disabled).toBe(true);
  expect(picker("Reasoning effort").disabled).toBe(true);
  expect(document.activeElement).toBe(buttons().find((button) => button.textContent === "Cancel"));
});

it("starts on what the session runs with, waits for a change, and sends only what changed", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { effort: "low" });
  const { request, onClose, onChangeModel } = await mount(session);

  expect(document.body.textContent).toContain("It restarts to take them and keeps its conversation.");
  expect(picker("Model").textContent).toContain("Opus");
  expect(picker("Reasoning effort").textContent).toContain("low");
  expect(confirm().disabled).toBe(true);

  await act(async () => picker("Model").click());
  await act(async () => options().find((option) => option.textContent === "Sonnet")!.click());
  // Changed and changed back is no change.
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "high")!.click());
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "low")!.click());
  await act(async () => confirm().click());

  // The dialog hands the change on and closes at once; the request itself is the app's to send and report.
  expect(onChangeModel).toHaveBeenCalledExactlyOnceWith("s-1", { model: "sonnet" });
  expect(onClose).toHaveBeenCalledOnce();
  expect(request.mock.calls.map(([body]) => body.type)).not.toContain("set_session_model");
});

it("clears a value to the agent's own default, and says an interrupted session takes it at its next resume", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "interrupted");
  const { onChangeModel } = await mount(session);

  expect(document.body.textContent).toContain("It is not running, so it takes them the next time it is resumed.");
  await act(async () => picker("Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
  await act(async () => confirm().click());

  expect(onChangeModel).toHaveBeenCalledExactlyOnceWith("s-1", { model: null });
});

it("does not offer Confirm while the session is starting", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "starting", { model: "sonnet" });
  const { onChangeModel } = await mount(session);

  await act(async () => picker("Model").querySelector<HTMLElement>('[data-slot="select-clear-button"]')!.click());
  expect(confirm().disabled).toBe(true);
  await act(async () => confirm().click());
  expect(onChangeModel).not.toHaveBeenCalled();
});

it("marks a stored value the list lacks only once the list is known", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { model: "retired", effort: "ultra" });
  await mount(session, "pending");
  expect(picker("Model").textContent).toContain("retired");
  expect(picker("Model").textContent).not.toContain("Not in the list");
  expect(picker("Reasoning effort").textContent).not.toContain("Not in the list");
});

it("marks a stored value a loaded list lacks", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { model: "retired", effort: "ultra" });
  await mount(session);
  expect(picker("Model").textContent).toContain("Not in the list");
  expect(picker("Reasoning effort").textContent).toContain("Not in the list");
});

it("leaves a stored value unmarked when the list could not be read, the warning mark saying why", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { model: "retired", effort: "ultra" });
  await mount(session, "unavailable");
  expect(picker("Model").textContent).toContain("retired");
  expect(picker("Model").textContent).not.toContain("Not in the list");
  expect(picker("Reasoning effort").textContent).not.toContain("Not in the list");
});

it("marks nothing as missing from a list that came back empty", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { model: "retired", effort: "ultra" });
  await mount(session, "empty");
  expect(picker("Model").textContent).toContain("retired");
  expect(picker("Model").textContent).not.toContain("Not in the list");
  expect(picker("Reasoning effort").textContent).not.toContain("Not in the list");
});

const setSession = (store: ReturnType<typeof createStateStore>, session: Session, extra: Partial<Session>) =>
  act(async () => store.setState({ sessions: new Map([[session.id, { ...session, ...extra }]]) }));

it("follows a change made to the session while it is open, for the fields the user has not edited", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { effort: "low" });
  const { onChangeModel, store } = await mount(session);
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "high")!.click());

  await setSession(store, session, { model: "sonnet", effort: "low" });
  expect(picker("Model").textContent).toContain("Sonnet");
  // The user's own edit stays.
  expect(picker("Reasoning effort").textContent).toContain("high");
  await act(async () => confirm().click());
  expect(onChangeModel).toHaveBeenCalledExactlyOnceWith("s-1", { effort: "high" });
});

it("recomputes what changed against the session's latest values", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { effort: "low" });
  const { store } = await mount(session);
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "high")!.click());
  expect(confirm().disabled).toBe(false);

  // Another window made the same change: nothing is left to do.
  await setSession(store, session, { effort: "high" });
  expect(confirm().disabled).toBe(true);
});

it.each([
  ["starting", { status: "starting" }, "The session is starting…"],
  ["switching its account", { switching_account: true }, "The session is switching accounts…"],
] as const)("keeps Confirm disabled when the session becomes %s while the dialog is open", async (_name, change, explanation) => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { effort: "low" });
  const { onChangeModel, store } = await mount(session);
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "high")!.click());
  expect(confirm().disabled).toBe(false);

  await setSession(store, session, change);
  expect(document.querySelector(".alert")?.textContent).toContain(explanation);
  expect(document.body.textContent).toContain("Wait for it to finish before changing the model settings.");
  expect(confirm().disabled).toBe(true);
  await act(async () => confirm().click());
  expect(onChangeModel).not.toHaveBeenCalled();
});

it("follows the session again for a field picked back to the session's value", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { effort: "low" });
  const { onChangeModel, store } = await mount(session);
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "high")!.click());
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "low")!.click());

  await setSession(store, session, { effort: "high" });
  expect(picker("Reasoning effort").textContent).toContain("high");
  expect(confirm().disabled).toBe(true);
  await act(async () => confirm().click());
  expect(onChangeModel).not.toHaveBeenCalled();
});

const failureModal = () => document.querySelector('[role="alertdialog"]');
const acknowledge = () => buttons().find((button) => button.textContent === "OK")!;

it("turns into a failure modal when the session is archived while the dialog is open", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working", { effort: "low" });
  const { onChangeModel, onClose, store } = await mount(session);
  await act(async () => picker("Reasoning effort").click());
  await act(async () => options().find((option) => option.textContent === "high")!.click());

  await setSession(store, session, { status: "archived" });
  expect(failureModal()?.textContent).toContain("Cannot change the model");
  expect(failureModal()?.querySelectorAll("[data-inline-reference]")).toHaveLength(1);
  expect(failureModal()?.querySelector("[data-inline-reference]")?.textContent).toBe("Fix search");
  expect(failureModal()?.textContent).toContain("was archived, so its model cannot be changed. Reopen it to change its model and reasoning effort.");
  expect(buttons().some((button) => button.textContent === "Change model")).toBe(false);
  expect(onChangeModel).not.toHaveBeenCalled();
  await act(async () => acknowledge().click());
  expect(onClose).toHaveBeenCalledOnce();
});

it("turns into a failure modal when the session is deleted while the dialog is open, even if nothing was edited", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle", { has_conversation: false });
  const { onChangeModel, store } = await mount(session);
  expect(failureModal()).toBeNull();

  await act(async () => store.setState({ sessions: new Map() }));
  expect(failureModal()?.textContent).toContain("no longer exists, so its model cannot be changed.");
  expect(failureModal()?.querySelectorAll("[data-inline-reference]")).toHaveLength(1);
  expect(failureModal()?.querySelector("[data-inline-reference]")?.textContent).toBe("Fix search");
  expect(buttons().some((button) => button.textContent === "Change model")).toBe(false);
  expect(onChangeModel).not.toHaveBeenCalled();
});

it("says a running session with no conversation yet starts a fresh one", async () => {
  await mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle", { has_conversation: false }));
  expect(document.body.textContent).toContain("with a fresh conversation, as nothing has been typed into it yet");
  expect(document.body.textContent).not.toContain("keeps its conversation");
});

it("says restarting a session waiting for the user ends the prompt it is showing", async () => {
  await mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "waiting_user"));
  expect(document.body.textContent).toContain("It is waiting for you, and restarting ends the prompt it is showing.");
});

it("does not mention a prompt for a session that is not waiting for the user", async () => {
  await mount(sessionOf("s-1", parentConsole.id, project.id, "Fix search", "working"));
  expect(document.body.textContent).not.toContain("It is waiting for you, and restarting ends the prompt it is showing.");
});

it("freezes the latest session name when a renamed session is deleted", async () => {
  const session = sessionOf("s-1", parentConsole.id, project.id, "Fix search", "idle");
  const { store } = await mount(session);
  await setSession(store, session, { title: "New title" });
  expect(document.querySelector('[role="dialog"] [data-inline-reference]')?.textContent).toBe("New title");
  await act(async () => store.setState({ sessions: new Map() }));
  expect(failureModal()?.querySelector("[data-inline-reference]")?.textContent).toBe("New title");
  expect(failureModal()?.textContent).not.toContain("Fix search");
});
