// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { fakeTimers } from "../test/clock";
import { providerLabel, settingMessageKey } from "../agents/providers";
import { accountOf, consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { translatorFor, type MessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { ClientRequestBody, Event } from "../protocol";
import { DaemonProvider, type SessionRequest } from "../store";
import { sessionRequestOf } from "../store/sessionRequests";
import { buttons, click, optionElements, pickerNamed, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { PendingQuestionDialog } from "./PendingQuestionDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const providers = TEST_PROVIDERS.filter((provider) => provider.settings.some((setting) => setting.compatibility_with !== null));
const provider = providers[0]; const other = providers[1];
const descriptor = provider.settings.find((entry) => entry.compatibility_with !== null)!;
const t = translatorFor("en");
const label = withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(settingMessageKey(provider.id, descriptor.id, "label")!, { agent: isolate(providerLabel(t, TEST_PROVIDERS, provider.id)) }))!;
const field = () => pickerNamed(label);
async function mount(action: "handoff" | "switch") {
  fakeTimers();
  const target = sessionOf("target", "c", "p", "Target", "idle", { agent: action === "handoff" ? other.id : provider.id, compatibility: { [descriptor.id]: true } });
  const request: SessionRequest = sessionRequestOf({ type: "session_request", request_id: "r", session: "asker", action, kind: "project", console: "c", project: "p", agent: provider.id, target: target.id, owner: null, reopened_owners: [], account: action === "switch" ? "receiving" : null, conversation: null, branches: [], compatibility: { [descriptor.id]: false }, requested_at: 1 })!;
  const owner = consoleOf("c", "Console"); const project = projectOf("p", "c", "Project", { launch: { [provider.id]: { compatibility: { [descriptor.id]: true } } } });
  const sent: ClientRequestBody[] = [];
  const daemon = testDaemon({ state: { consoles: new Map([[owner.id, owner]]), projects: new Map([[project.id, project]]), sessions: new Map([[target.id, target]]), sessionRequests: [request] }, request: async (body): Promise<Event> => {
    sent.push(body);
    if (body.type === "list_agent_models") return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [], unavailable: null };
    if (body.type === "answer_session_request") return { type: "ack", id: null };
    throw new Error(`Unexpected request: ${body.type}`);
  } });
  daemon.store.setState((state) => ({ settings: { ...state.settings, accounts: [accountOf("receiving", provider.id, "Receiving", "/receiving")] } }));
  const container = document.body.appendChild(document.createElement("div")); const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await act(async () => root.render(<DaemonProvider value={daemon}><PendingQuestionDialog /></DaemonProvider>));
  return { sent };
}
it("carries a handoff event's held compatibility through the visible form into the approval", async () => {
  const { sent } = await mount("handoff");
  await click(buttons().find((entry) => entry.textContent === "Advanced"));
  expect(field()).toBeDefined(); expect(field().textContent).toBe("Off");
  const confirm = buttons().find((entry) => entry.textContent === "Hand off session"); expect(confirm).toBeDefined(); await click(confirm);
  expect(sent.find((body) => body.type === "answer_session_request")).toMatchObject({ approve: true, request_id: "r", agent: provider.id, compatibility: { [descriptor.id]: false } });
});
it("lets a switch request return a session's held compatibility to follow and sends null in its answer", async () => {
  const { sent } = await mount("switch");
  await click(buttons().find((entry) => entry.textContent === "Advanced"));
  expect(field()).toBeDefined(); expect(field().textContent).toBe("On");
  await click(field()); await click(optionElements().find((entry) => entry.textContent?.startsWith("Same as project")));
  const confirm = buttons().find((entry) => entry.textContent === "Switch account"); expect(confirm).toBeDefined(); await click(confirm);
  expect(sent.find((body) => body.type === "answer_session_request")).toMatchObject({ approve: true, request_id: "r", account: "receiving", compatibility: { [descriptor.id]: null } });
});
