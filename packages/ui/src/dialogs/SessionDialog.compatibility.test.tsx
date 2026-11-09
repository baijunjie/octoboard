// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../test/clock";
import { providerLabel, settingMessageKey } from "../agents/providers";
import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { translatorFor, type MessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { ClientRequestBody, Event } from "../protocol";
import { DaemonProvider } from "../store";
import { buttons, click, inputLabelled, optionElements, pickerNamed, type, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { SessionDialog } from "./SessionDialog";

vi.mock("../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const provider = TEST_PROVIDERS.find((entry) => entry.settings.some((setting) => setting.compatibility_with !== null))!;
const setting = provider.settings.find((entry) => entry.compatibility_with !== null)!;
const t = translatorFor("en");
const name = withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(settingMessageKey(provider.id, setting.id, "label")!, { agent: isolate(providerLabel(t, TEST_PROVIDERS, provider.id)) }))!;
const field = () => pickerNamed(name);
const button = (name: string) => { const found = buttons().find((entry) => entry.textContent === name); expect(found).toBeDefined(); return found!; };
const UUID = "11111111-2222-3333-4444-555555555555";
async function mount(kind: "project" | "console" = "project", locate: "record" | "archived" = "record") {
  fakeTimers();
  const owner = consoleOf("c", "Console", { default_agent: provider.id, launch: { [provider.id]: { compatibility: { [setting.id]: true } } }, project_session_launch: { [provider.id]: { compatibility: { [setting.id]: true } } } });
  const project = projectOf("p", "c", "Project", { default_agent: provider.id, launch: { [provider.id]: { compatibility: { [setting.id]: false } } } });
  const existing = sessionOf("existing", owner.id, project.id, "Existing", "archived", { agent: provider.id, agent_session_id: UUID, compatibility: { [setting.id]: true } });
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({ state: { consoles: new Map([[owner.id, owner]]), projects: new Map([[project.id, project]]), sessions: new Map([[existing.id, existing]]) }, request: async (body): Promise<Event> => {
    sent.push(body);
    if (body.type === "list_agent_models") return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [], unavailable: null };
    if (body.type === "locate_conversation_to_resume") return { type: "conversation_located", id: null, location: locate === "record" ? { kind: "record", agent: provider.id, account: null } : { kind: "archived_session", session: existing.id } };
    if (body.type === "open_session") return { type: "session_opened", id: null, session: sessionOf("opened", owner.id, kind === "project" ? project.id : undefined, "New", "idle", { agent: provider.id }) };
    throw new Error(`Unexpected request: ${body.type}`);
  } });
  const container = document.body.appendChild(document.createElement("div")); const root = createRoot(container); const resume = vi.fn(async () => {});
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await act(async () => root.render(<DaemonProvider value={daemon}><SessionDialog console={owner} project={kind === "project" ? project : undefined} sessions={[existing]} binding={{ kind: "unbound" }} onClose={() => {}} onOpened={() => {}} onResume={resume} /></DaemonProvider>));
  return { sent, resume, daemon, project, owner };
}
it.each(["project", "console"] as const)("opens a new %s session with the compatibility override picked in Advanced", async (kind) => {
  const { sent } = await mount(kind); await click(button("Advanced"));
  expect(field()).toBeDefined(); expect(field().textContent).toContain(kind === "project" ? "Same as project, Off" : "Same as console, On");
  await click(field()); await click(optionElements().find((entry) => entry.textContent === "Off"));
  await click(button("Open"));
  expect(sent.find((body) => body.type === "open_session")).toMatchObject({ compatibility: { [setting.id]: false } });
});
it.each(["record", "archived"] as const)("offers compatibility only when resuming an external record: %s", async (locate) => {
  const { sent, resume } = await mount("project", locate);
  await click([...document.querySelectorAll('[role="tab"]')].find((entry) => entry.textContent === "Resume conversation"));
  type(inputLabelled("Session ID"), UUID);
  while (!sent.some((body) => body.type === "locate_conversation_to_resume")) {
    await act(async () => { await vi.advanceTimersToNextTimerAsync(); });
  }
  if (locate === "record") {
    await click(button("Advanced")); expect(field()).toBeDefined();
    expect(field().textContent).toContain("Same as project, Off");
    await click(field()); await click(optionElements().find((entry) => entry.textContent === "On"));
    await click(button("Open"));
    expect(sent.find((body) => body.type === "open_session")).toMatchObject({ resume_agent_session_id: UUID, compatibility: { [setting.id]: true } });
  } else {
    expect(field()).toBeUndefined();
    expect(buttons().some((entry) => entry.textContent === "Advanced")).toBe(false);
    await click(button(t("dialog.session.reopen"))); expect(resume).toHaveBeenCalledExactlyOnceWith("existing");
    expect(sent.some((body) => body.type === "open_session")).toBe(false);
  }
});
