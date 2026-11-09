// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { fakeTimers } from "../test/clock";
import { providerLabel, settingMessageKey } from "../agents/providers";
import { CodedError } from "../errors/codedError";
import { consoleOf } from "../fixtures/builders";
import { translatorFor, type MessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { ClientRequestBody, Event, Console } from "../protocol";
import { DaemonProvider } from "../store";
import { buttons, click, optionElements, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { ConsoleDialog } from "./ConsoleDialog";
import { nameOf } from "./consoleDialogTestSupport";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const providers = TEST_PROVIDERS.filter((provider) => provider.settings.some((setting) => setting.compatibility_with !== null));
const [first, second] = providers;
const setting = (provider = first) => provider.settings.find((entry) => entry.compatibility_with !== null)!;
const t = translatorFor("en");
const name = (provider = first) => withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(settingMessageKey(provider.id, setting(provider).id, "heading")!, { agent: isolate(providerLabel(t, TEST_PROVIDERS, provider.id)) }))!;
const field = (provider = first, group = "Console sessions") => buttons().find((entry) => withoutIsolates(nameOf(entry).join(" ")) === `${group} ${name(provider)}`)!;
const path = (provider = first) => `launch.${provider.id}.compatibility.${setting(provider).id}`;
const button = (text: string) => { const found = buttons().find((entry) => entry.textContent === text); expect(found).toBeDefined(); return found!; };
async function pick(value: string, provider = first) {
  expect(field(provider)).toBeDefined(); await click(field(provider));
  const option = optionElements().find((entry) => entry.textContent?.startsWith(value)); expect(option).toBeDefined(); await click(option);
}
async function mount(owner = consoleOf("c", "Console", { default_agent: first.id })) {
  fakeTimers();
  const pending: { body: ClientRequestBody; resolve: (event: Event) => void; reject: (error: Error) => void }[] = [];
  const daemon = testDaemon({ state: { consoles: new Map([[owner.id, owner]]) }, request: async (body) => {
    if (body.type === "list_agent_models") return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [], unavailable: null };
    return new Promise<Event>((resolve, reject) => pending.push({ body, resolve, reject }));
  } });
  const container = document.body.appendChild(document.createElement("div")); const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount()); container.remove();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><ConsoleDialog console={owner} onClose={() => {}} /></DaemonProvider>));
  return { daemon, pending, owner };
}

it("offers separate console and project columns with agent-level follow badges that update live", async () => {
  const { daemon } = await mount(); await click(button("Advanced"));
  const helpLabel = withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(settingMessageKey(first.id, setting().id, "helpLabel")!, { agent: isolate(providerLabel(t, TEST_PROVIDERS, first.id)) }));
  const help = buttons().find((entry) => withoutIsolates(entry.getAttribute("aria-label")) === helpLabel);
  expect(help).toBeDefined();
  expect(help!.closest('[aria-hidden="true"]')).toBeNull();
  for (const group of ["Console sessions", "Project sessions"]) {
    expect(field(first, group)).toBeDefined();
    expect(field(first, group).textContent).toContain("Same as agent setting");
  }
  act(() => daemon.store.setState((state) => ({ settings: { ...state.settings, provider_settings: { [first.id]: { [setting().id]: false } } } })));
  for (const group of ["Console sessions", "Project sessions"]) expect(field(first, group).textContent).toContain("Same as agent setting, Off");
  act(() => daemon.store.setState((state) => ({ settings: { ...state.settings, provider_settings: { [first.id]: { [setting().id]: true } } } })));
  for (const group of ["Console sessions", "Project sessions"]) expect(field(first, group).textContent).toContain("Same as agent setting, On");
});

it("saves the two launch sets independently with null for follow and only changed field versions", async () => {
  const projectPath = `project_session_launch.${first.id}.compatibility.${setting().id}`;
  const owner = consoleOf("c", "Console", { default_agent: first.id, launch: { [first.id]: { compatibility: { [setting().id]: false } } }, versions: { [path()]: 3, [projectPath]: 6, [path(second)]: 9 } });
  const { pending } = await mount(owner); await click(button("Advanced"));
  await pick("Same as agent setting");
  await click(field(first, "Project sessions")); await click(optionElements().find((entry) => entry.textContent === "Off"));
  await click([...document.querySelectorAll('[role="tab"]')].find((entry) => entry.textContent === providerLabel(t, TEST_PROVIDERS, second.id)));
  expect(field()).toBeUndefined(); await pick("On", second);
  await click(button("Save"));
  expect(pending[0].body).toEqual({ type: "update_console", console: owner.id,
    launch: { [first.id]: { compatibility: { [setting().id]: null } }, [second.id]: { compatibility: { [setting(second).id]: true } } },
    project_session_launch: { [first.id]: { compatibility: { [setting().id]: false } } },
    base_versions: { [path()]: 3, [path(second)]: 9, [projectPath]: 6 } });
});

it("resets a conflicting compatibility switch to the broadcast value and preserves another column's edit", async () => {
  const projectPath = `project_session_launch.${first.id}.compatibility.${setting().id}`;
  const owner = consoleOf("c", "Console", { default_agent: first.id, versions: { [path()]: 1, [projectPath]: 2 } });
  const { daemon, pending } = await mount(owner); await click(button("Advanced")); await pick("On");
  await click(field(first, "Project sessions")); await click(optionElements().find((entry) => entry.textContent === "Off"));
  await click(button("Save"));
  await act(async () => pending[0].reject(new CodedError("changed elsewhere", "write_conflict", { fields: path(), versions: JSON.stringify({ [path()]: 5 }) })));
  expect(field().textContent).toBe("On");
  const latest: Console = { ...owner, versions: { ...owner.versions, [path()]: 5 }, launch: { [first.id]: { model: null, effort: null, compatibility: { [setting().id]: false } } } };
  await act(async () => daemon.store.setState({ consoles: new Map([[owner.id, latest]]) }));
  expect(field().textContent).toBe("Off");
  expect(field(first, "Project sessions").textContent).toBe("Off");
  await click(button("Save"));
  expect(pending[1].body).toEqual({ type: "update_console", console: owner.id, project_session_launch: { [first.id]: { compatibility: { [setting().id]: false } } }, base_versions: { [projectPath]: 2 } });
});
