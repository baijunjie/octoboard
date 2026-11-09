// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished } from "vitest";

import { fakeTimers } from "../test/clock";
import { providerLabel, settingMessageKey } from "../agents/providers";
import { CodedError } from "../errors/codedError";
import { consoleOf, projectOf } from "../fixtures/builders";
import { translatorFor, type MessageKey } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { ClientRequestBody, Event, Project } from "../protocol";
import { DaemonProvider } from "../store";
import { buttons, click, inputLabelled, optionElements, pickerNamed, type, withoutIsolates } from "../test/dom";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { ProjectDialog } from "./ProjectDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
const providers = TEST_PROVIDERS.filter((provider) => provider.settings.some((setting) => setting.compatibility_with !== null));
const [first, second] = providers;
const setting = (provider = first) => provider.settings.find((entry) => entry.compatibility_with !== null)!;
const t = translatorFor("en");
const name = (provider = first) => withoutIsolates((t as (key: MessageKey, params: Record<string, string>) => string)(settingMessageKey(provider.id, setting(provider).id, "label")!, { agent: isolate(providerLabel(t, TEST_PROVIDERS, provider.id)) }))!;
const field = (provider = first) => pickerNamed(name(provider));
const path = (provider = first) => `launch.${provider.id}.compatibility.${setting(provider).id}`;
const button = (text: string) => { const found = buttons().find((entry) => entry.textContent === text); expect(found).toBeDefined(); return found!; };
async function pick(value: string, provider = first) {
  expect(field(provider)).toBeDefined(); await click(field(provider));
  const option = optionElements().find((entry) => entry.textContent?.startsWith(value)); expect(option).toBeDefined(); await click(option);
}
async function mount(project = projectOf("p", "c", "Project", { default_agent: first.id }), inherited = false, create = false) {
  fakeTimers();
  const owner = consoleOf("c", "Console", { project_session_launch: { [first.id]: { compatibility: { [setting().id]: inherited } } } });
  const pending: { body: ClientRequestBody; resolve: (event: Event) => void; reject: (error: Error) => void }[] = [];
  const daemon = testDaemon({ state: { consoles: new Map([[owner.id, owner]]), projects: new Map([[project.id, project]]) }, request: async (body) => {
    if (body.type === "list_agent_models") return { type: "agent_models", id: null, agent: body.agent, account: body.account ?? null, models: [], unavailable: null };
    return new Promise<Event>((resolve, reject) => pending.push({ body, resolve, reject }));
  } });
  const container = document.body.appendChild(document.createElement("div")); const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount()); container.remove();
  });
  await act(async () => root.render(<DaemonProvider value={daemon}><ProjectDialog consoleId={owner.id} project={create ? undefined : project} onClose={() => {}} /></DaemonProvider>));
  return { daemon, pending, owner };
}

it("puts the selected agent's switches in Launch settings, keeps hidden-agent edits and saves only changed switches with their versions", async () => {
  const project = projectOf("p", "c", "Project", { default_agent: first.id, versions: { [path()]: 7, [path(second)]: 9 } });
  const { pending } = await mount(project);
  await click(button("Advanced"));
  const tabs = [...document.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent);
  expect(tabs).toContain("Launch settings"); expect(tabs).not.toContain("Agent compatibility");
  expect(field().textContent).toContain("Same as console, Off");
  await pick("On");
  await click(pickerNamed("Default agent"));
  await click(optionElements().find((entry) => entry.textContent === providerLabel(t, TEST_PROVIDERS, second.id)));
  expect(field()).toBeUndefined(); expect(field(second)).toBeDefined();
  await pick("Off", second);
  await click(button("Save"));
  expect(pending[0].body).toMatchObject({ type: "update_project", launch: { [first.id]: { compatibility: { [setting().id]: true } }, [second.id]: { compatibility: { [setting(second).id]: false } } }, base_versions: { [path()]: 7, [path(second)]: 9 } });
});

it("clears a held switch with null and follows live console and agent changes", async () => {
  const project = projectOf("p", "c", "Project", { default_agent: first.id, launch: { [first.id]: { compatibility: { [setting().id]: true } } }, versions: { [path()]: 4 } });
  const { daemon, pending, owner } = await mount(project);
  await click(button("Advanced")); await pick("Same as console");
  expect(field().textContent).toContain("Same as console, Off");
  act(() => daemon.store.setState({ consoles: new Map([[owner.id, { ...owner, project_session_launch: {} }]]) }));
  act(() => daemon.store.setState((state) => ({ settings: { ...state.settings, provider_settings: { [first.id]: { [setting().id]: true } } } })));
  expect(field().textContent).toContain("Same as console, On");
  await click(button("Save"));
  expect(pending[0].body).toEqual({ type: "update_project", project: project.id, launch: { [first.id]: { compatibility: { [setting().id]: null } } }, base_versions: { [path()]: 4 } });
});

it("shows an unedited switch changed elsewhere and does not send it with another edit", async () => {
  const project = projectOf("p", "c", "Project", { default_agent: first.id, versions: { [path()]: 1, name: 1 } });
  const { daemon, pending } = await mount(project);
  await click(button("Advanced"));
  act(() => daemon.store.setState({ projects: new Map([[project.id, { ...project, versions: { ...project.versions, [path()]: 2 }, launch: { [first.id]: { model: null, effort: null, compatibility: { [setting().id]: false } } } }]]) }));
  expect(field().textContent).toBe("Off");
  type(inputLabelled("Name"), "Renamed"); await click(button("Save"));
  expect(pending[0].body).toEqual({ type: "update_project", project: project.id, name: "Renamed", base_versions: { name: 1 } });
});

it("waits for a conflict's broadcast, restores that switch and retains an unrelated agent's edit", async () => {
  const project = projectOf("p", "c", "Project", { default_agent: first.id, versions: { [path()]: 1, [path(second)]: 1 } });
  const { daemon, pending } = await mount(project);
  await click(button("Advanced")); await pick("On");
  await click(pickerNamed("Default agent")); await click(optionElements().find((entry) => entry.textContent === providerLabel(t, TEST_PROVIDERS, second.id))); await pick("Off", second);
  await click(button("Save"));
  await act(async () => pending[0].reject(new CodedError("changed elsewhere", "write_conflict", { fields: path(), versions: JSON.stringify({ [path()]: 5 }) })));
  const latest: Project = { ...project, versions: { ...project.versions, [path()]: 5 }, launch: { [first.id]: { model: null, effort: null, compatibility: { [setting().id]: false } } } };
  await act(async () => daemon.store.setState({ projects: new Map([[project.id, latest]]) }));
  expect(field(second).textContent).toBe("Off");
  await click(button("Save"));
  expect(pending[1].body).toMatchObject({ launch: { [second.id]: { compatibility: { [setting(second).id]: false } } } });
  expect((pending[1].body as { launch: object }).launch).not.toHaveProperty(first.id);
});

it("shows only Git workflow under Detect from files, with no launch or compatibility tab", async () => {
  await mount(undefined, false, true);
  await click(buttons().find((entry) => entry.textContent?.includes("A single directory")));
  const parent = optionElements().find((entry) => entry.textContent?.startsWith("A parent directory"));
  expect(parent).toBeDefined(); await click(parent);
  await click(button("Advanced"));
  expect(pickerNamed("Model")).toBeUndefined();
  for (const provider of providers) expect(field(provider)).toBeUndefined();
  expect(pickerNamed("Where sessions develop")).toBeDefined();
  expect([...document.querySelectorAll('[role="tab"]')].map((tab) => tab.textContent)).toEqual(["Git workflow"]);
});
