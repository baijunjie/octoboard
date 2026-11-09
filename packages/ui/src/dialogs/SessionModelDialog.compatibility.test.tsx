// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { fakeTimers } from "../test/clock";
import { settingMessageKey, providerLabel } from "../agents/providers";
import { consoleOf, projectOf, sessionOf } from "../fixtures/builders";
import { translatorFor, type MessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import { DaemonProvider } from "../store";
import { buttons, click, optionElements, pickerNamed, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { SessionModelDialog } from "./SessionModelDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const provider = TEST_PROVIDERS.find((entry) => entry.settings.filter((setting) => setting.compatibility_with !== null).length >= 2)!;
const settings = provider.settings.filter((setting) => setting.compatibility_with !== null);
const [first, second] = settings;
const t = translatorFor("en");
const field = (id = first.id) => pickerNamed(withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(settingMessageKey(provider.id, id, "label")!, { agent: isolate(providerLabel(t, TEST_PROVIDERS, provider.id)) }))!);
const confirm = () => buttons().find((entry) => entry.textContent === "Change model")!;
async function pick(text: string, id = first.id) {
  expect(field(id)).toBeDefined(); await click(field(id));
  const option = optionElements().find((entry) => entry.textContent?.startsWith(text)); expect(option).toBeDefined(); await click(option);
}
async function mount(kind: "project" | "console" = "project", held: Record<string, boolean> = {}) {
  fakeTimers();
  const session = sessionOf("s", "c", kind === "project" ? "p" : undefined, "Session", "idle", { agent: provider.id, compatibility: held });
  const owner = consoleOf("c", "Console", { launch: { [provider.id]: { compatibility: { [first.id]: true } } }, project_session_launch: { [provider.id]: { compatibility: { [first.id]: true, [second.id]: false } } } });
  const project = projectOf("p", "c", "Project", { launch: { [provider.id]: { compatibility: { [first.id]: false } } } });
  const daemon = testDaemon({ state: { consoles: new Map([[owner.id, owner]]), projects: new Map([[project.id, project]]), sessions: new Map([[session.id, session]]) }, request: async (body) => {
    if (body.type === "list_agent_models") return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [], unavailable: null };
    throw new Error(`Unexpected request: ${body.type}`);
  } });
  const changed = vi.fn(); const container = document.body.appendChild(document.createElement("div")); const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  await act(async () => root.render(<DaemonProvider value={daemon}><SessionModelDialog session={session} onClose={() => {}} onChangeModel={changed} /></DaemonProvider>));
  return { changed, daemon, session, project, owner };
}
it.each(["project", "console"] as const)("shows the %s session's follow level and its live effective value", async (kind) => {
  const { daemon, project, owner } = await mount(kind);
  expect(confirm().disabled).toBe(true);
  expect(field().textContent).toContain(kind === "project" ? "Same as project, Off" : "Same as console, On");
  if (kind === "project") {
    act(() => daemon.store.setState({ projects: new Map([[project.id, { ...project, launch: {} }]]) }));
    expect(field().textContent).toContain("Same as project, On");
  }
  act(() => daemon.store.setState({ projects: new Map(), consoles: new Map([[owner.id, { ...owner, launch: {}, project_session_launch: {} }]]) }));
  act(() => daemon.store.setState((state) => ({ settings: { ...state.settings, provider_settings: { [provider.id]: { [first.id]: false } } } })));
  expect(field().textContent).toContain(kind === "project" ? "Same as project, Off" : "Same as console, Off");
});
it("sends a difference from held session values, including null when an inherited value happens to match", async () => {
  const { changed, session } = await mount("project", { [first.id]: false, [second.id]: true });
  expect(field().textContent).toBe("Off");
  await pick("Same as project");
  await click(confirm());
  expect(changed).toHaveBeenCalledExactlyOnceWith(session.id, { compatibility: { [first.id]: null } });
});
it("keeps an earlier compatibility edit while another switch changes and omits switches returned to their held value", async () => {
  const { changed, session } = await mount("project", { [first.id]: true, [second.id]: true });
  await pick("Off"); await pick("Off", second.id);
  await pick("On", first.id);
  await click(confirm());
  expect(changed).toHaveBeenCalledExactlyOnceWith(session.id, { compatibility: { [second.id]: false } });
});
it("follows session updates for an unedited switch and avoids restarting when another window made the same edit", async () => {
  const { daemon, session } = await mount("project", { [first.id]: true });
  act(() => daemon.store.setState({ sessions: new Map([[session.id, { ...session, compatibility: { [first.id]: false } }]]) }));
  expect(field().textContent).toBe("Off");
  await pick("On"); expect(confirm().disabled).toBe(false);
  act(() => daemon.store.setState({ sessions: new Map([[session.id, { ...session, compatibility: { [first.id]: true } }]]) }));
  expect(confirm().disabled).toBe(true);
});
