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
import { buttons, click, optionElements, pickerNamed, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { HandoffDialog } from "./HandoffDialog";

vi.mock("../preferences/theme", () => ({ useOctoboardTheme: () => ({ resolved: "dark" }) }));
(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const provider = TEST_PROVIDERS.find((entry) => entry.settings.some((setting) => setting.compatibility_with !== null))!;
const setting = provider.settings.find((entry) => entry.compatibility_with !== null)!;
const t = translatorFor("en");
const name = withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(settingMessageKey(provider.id, setting.id, "label")!, { agent: isolate(providerLabel(t, TEST_PROVIDERS, provider.id)) }))!;
const field = () => pickerNamed(name);
const button = (name: string) => { const found = buttons().find((entry) => entry.textContent === name); expect(found).toBeDefined(); return found!; };
async function mount(kind: "project" | "console" = "project") {
  fakeTimers();
  const owner = consoleOf("c", "Console", { default_agent: provider.id, launch: { [provider.id]: { compatibility: { [setting.id]: true } } }, project_session_launch: { [provider.id]: { compatibility: { [setting.id]: true } } } });
  const project = projectOf("p", "c", "Project", { default_agent: provider.id, launch: { [provider.id]: { compatibility: { [setting.id]: false } } } });
  const original = TEST_PROVIDERS.find((entry) => entry.id !== provider.id)!;
  const existing = sessionOf("existing", owner.id, kind === "project" ? project.id : undefined, "Existing", "idle", { agent: original.id });
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({ state: { consoles: new Map([[owner.id, owner]]), projects: new Map([[project.id, project]]), sessions: new Map([[existing.id, existing]]) }, request: async (body): Promise<Event> => {
    sent.push(body);
    if (body.type === "list_agent_models") return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [], unavailable: null };
    if (body.type === "inspect_session_branches") return { type: "session_branches_inspected", id: null, items: [] };
    if (body.type === "handoff_session") return { type: "session_opened", id: null, session: sessionOf("receiving", owner.id, kind === "project" ? project.id : undefined, "New", "idle", { agent: provider.id }) };
    throw new Error(`Unexpected request: ${body.type}`);
  } });
  const container = document.body.appendChild(document.createElement("div")); const root = createRoot(container); const resume = vi.fn(async () => {});
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await act(async () => root.render(<DaemonProvider value={daemon}><HandoffDialog session={existing} onClose={() => {}} onHandedOff={resume} /></DaemonProvider>));
  return { sent, resume, daemon, project, owner };
}
it.each(["project", "console"] as const)("hands off a %s session with the receiving agent's compatibility override", async (kind) => {
  const { sent, resume } = await mount(kind); await click(button("Advanced"));
  expect(field()).toBeDefined(); expect(field().textContent).toContain(kind === "project" ? "Same as project, Off" : "Same as console, On");
  await click(field()); await click(optionElements().find((entry) => entry.textContent === "Off"));
  await click(button("Hand off"));
  expect(sent.find((body) => body.type === "handoff_session")).toMatchObject({ agent: provider.id, compatibility: { [setting.id]: false } });
  expect(resume).toHaveBeenCalledExactlyOnceWith("receiving");
});
