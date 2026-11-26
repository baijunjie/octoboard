// @vitest-environment jsdom
// How the window hosts the message composer: below the agent terminal while this client has it on and the daemon keeps a
// queue, the compact queue line over the terminal while it is off, the F6 cycle through both, and the end of an edit
// when the switch goes off.
import { act } from "react";
import { expect, it, vi } from "vitest";

import { byLabel, fakeDaemon, mountApp, pressElement, pressMenuItem, row, stubWindow } from "./appTestSupport";
import { sessionComposerEnabled } from "./composer/composerSetting";
import { composerDraft } from "./composer/composerDrafts";
import { consoleOf, projectOf, queuedMessage, sessionOf, snapshotState } from "./fixtures/builders";
import { isolate } from "./i18n/isolate";
import type { ClientRequestBody, QueuedMessage } from "./protocol";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
// Each test mounts the whole App, which a machine under load can take longer than the default 5s for.
vi.setConfig({ testTimeout: 20_000 });

vi.mock("./report/ReportPanel", () => ({ ReportPanel: () => null }));
vi.mock("./project-browser/LazyProjectBrowser", () => ({ LazyProjectBrowser: () => null }));
// The agent terminal's input stands in for xterm's.
vi.mock("./terminal/TerminalController", () => import("./terminalControllerTestSupport"));

const console_ = consoleOf("c-main", "Main");
const project = projectOf("p-site", console_.id, "Website");
const live = sessionOf("s-live", console_.id, project.id, "Fix header", "idle");
const archived = sessionOf("s-old", console_.id, project.id, "Old fix", "archived");
const QUEUE: QueuedMessage[] = [
  queuedMessage({ id: "m1", text: "first", mode: "when_idle" }),
  queuedMessage({ id: "m2", text: "second", mode: "manual" }),
];

const composer = () => document.querySelector<HTMLElement>("[data-region=composer]");
const queueLine = () => document.querySelector<HTMLElement>("[data-region=queue]");
const terminal = () => document.querySelector<HTMLElement>("[data-region=terminal]");

async function mount({ composerOn = false, queues = { [live.id]: QUEUE } as Record<string, QueuedMessage[]> | null } = {}) {
  stubWindow();
  sessionComposerEnabled.set(composerOn);
  const requests: ClientRequestBody[] = [];
  const daemon = fakeDaemon(snapshotState({ consoles: [console_], projects: [project], sessions: [archived, live], messageQueues: queues }), (body) => {
    requests.push(body);
    return body.type.endsWith("_queued_message_mode") ? { type: "ack", id: null } : undefined;
  });
  mountApp(daemon);
  await pressElement(row(live.title));
  return { daemon, requests };
}

it("shows no composer while it is off, and the compact line of the queued messages over the terminal instead", async () => {
  await mount();
  expect(composer()).toBeNull();
  expect(queueLine()?.textContent).toContain("2 messages queued");
  expect(terminal()?.contains(queueLine())).toBe(true);
});

it("shows the composer below the terminal while it is on, with the queue in it and no compact line", async () => {
  await mount({ composerOn: true });
  expect(queueLine()).toBeNull();
  const shown = composer()!;
  expect(shown).not.toBeNull();
  expect(shown.querySelectorAll("[data-queued]")).toHaveLength(2);
  expect(terminal()!.contains(shown)).toBe(false);
  // The composer follows the terminal in the page.
  expect(terminal()!.compareDocumentPosition(shown) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
});

it("shows neither where the daemon keeps no composer queue, the switch being on", async () => {
  await mount({ composerOn: true, queues: null });
  expect(composer()).toBeNull();
  expect(queueLine()).toBeNull();
});

it("follows the switch while the window is open", async () => {
  await mount();
  expect(composer()).toBeNull();
  act(() => sessionComposerEnabled.set(true));
  expect(composer()).not.toBeNull();
  expect(queueLine()).toBeNull();
  act(() => sessionComposerEnabled.set(false));
  expect(composer()).toBeNull();
  expect(queueLine()?.textContent).toContain("2 messages queued");
});

it("keeps a draft per session, so the text written is still there once the composer is shown again", async () => {
  const { daemon } = await mount({ composerOn: true });
  const field = () => composer()!.querySelector<HTMLTextAreaElement>("textarea")!;
  const set = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  act(() => {
    set.call(field(), "half a message");
    field().dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(composerDraft(daemon.store, live.id).text).toBe("half a message");
  act(() => sessionComposerEnabled.set(false));
  act(() => sessionComposerEnabled.set(true));
  expect(field().value).toBe("half a message");
});

it("takes F6 from the terminal to the composer, and the compact line before the terminal while the composer is off", async () => {
  await mount({ composerOn: true });
  const f6 = (shiftKey = false) => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "F6", shiftKey, bubbles: true })));
  const input = terminal()!.querySelector<HTMLElement>("textarea")!;
  act(() => input.focus());
  f6();
  expect(document.activeElement).toBe(composer()!.querySelector("textarea"));
  f6(true);
  expect(terminal()!.contains(document.activeElement)).toBe(true);
  act(() => sessionComposerEnabled.set(false));
  act(() => terminal()!.querySelector<HTMLElement>("textarea")!.focus());
  f6(true);
  expect(queueLine()!.contains(document.activeElement)).toBe(true);
  f6();
  expect(terminal()!.contains(document.activeElement)).toBe(true);
});

it("keeps the composer out of reach while the archive view covers the agent area, as it does the terminal", async () => {
  await mount({ composerOn: true });
  await pressElement(byLabel(`Actions for project ${isolate(project.name)}`));
  await pressMenuItem("View archive");
  await pressMenuItem("View all");
  expect(composer()!.closest("[inert]")).not.toBeNull();
});

it("ends an edit of a queued message in this window when the switch goes off, giving the message its mode back", async () => {
  const { daemon, requests } = await mount({ composerOn: true });
  const menu = composer()!.querySelector<HTMLElement>('[data-queued="m1"] button[aria-label="Message actions"]');
  await pressElement(menu);
  await pressMenuItem("Edit");
  expect(composerDraft(daemon.store, live.id).editing?.message).toBe("m1");
  act(() => sessionComposerEnabled.set(false));
  expect(composerDraft(daemon.store, live.id).editing).toBeUndefined();
  expect(requests.filter((body) => body.type === "set_queued_message_mode").map((body) => (body as { mode: string }).mode)).toEqual(["manual", "when_idle"]);
});
