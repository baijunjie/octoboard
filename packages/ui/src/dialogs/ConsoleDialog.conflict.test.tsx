// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { errorAlerts } from "../components/alerts/errorAlerts";
import { CodedError } from "../errors/codedError";
import { accountOf, CONSOLE_VERSIONS, consoleOf } from "../fixtures/builders";
import type { Account, ClientRequestBody, Event } from "../protocol";
import { inputLabelled } from "../test/dom";
import { choose, click, clickButton, groupPicker, lastStore, openAdvanced, picker, render, sent, type } from "./consoleDialogTestSupport";

const codexWork: Account = accountOf("a-codex", "codex", "Codex work", "/home/dev/.codex-work");
const codexSpare: Account = accountOf("a-spare", "codex", "Codex spare", "/home/dev/.codex-spare");
const conflict = new CodedError("changed elsewhere", "write_conflict", { fields: "name", versions: '{"name":8}' });

it("sends only the fields that changed, each with the version the form read", async () => {
  const request = render(consoleOf("c-1", "Main", { project_session_agent: "grok" }));
  type("Name", "Renamed");
  await click(picker("Console sessions"));
  await choose("Codex");
  await clickButton("Save");
  expect(sent(request)).toEqual([
    { type: "update_console", console: "c-1", name: "Renamed", default_agent: "codex", base_versions: { name: 1, default_agent: 1 } },
  ]);
});

/** The store's console as another client left it, with `moved` at a newer version. */
const elsewhere = (name: string, moved: Record<string, number>, extra: Parameters<typeof consoleOf>[2] = {}) =>
  act(() => lastStore.setState({ consoles: new Map([["c-1", consoleOf("c-1", name, { versions: { ...CONSOLE_VERSIONS, ...moved }, ...extra })]]) }));

it("replaces a field with the current value as soon as the store shows a newer version of it, and bases the write on that", async () => {
  const request = render(consoleOf("c-1", "Main"));
  type("Name", "Mine");
  elsewhere("Theirs", { name: 5 });
  expect(inputLabelled("Name").value).toBe("Theirs");
  type("Name", "Renamed");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Renamed", base_versions: { name: 5 } }]);
});

it("keeps an edit when a version moves over the value the form read, and bases the write on the new version", async () => {
  const request = render(consoleOf("c-1", "Main"));
  type("Name", "Mine");
  elsewhere("Main", { name: 5 });
  expect(inputLabelled("Name").value).toBe("Mine");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Mine", base_versions: { name: 5 } }]);
});

it("replaces an edited account with the current one, and sends nothing for it", async () => {
  const request = render(consoleOf("c-1", "Main"), [codexWork, codexSpare]);
  await click(picker("Codex account"));
  await choose("Codex work");
  expect(picker("Codex account").textContent).toContain("Codex work");
  elsewhere("Main", { "accounts.codex": 3 }, { accounts: { codex: "a-spare" } });
  expect(picker("Codex account").textContent).toContain("Codex spare");
  type("Name", "Renamed");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Renamed", base_versions: { name: 1 } }]);
});

it("replaces an edited launch setting with the current one, and sends nothing for it", async () => {
  const request = render(consoleOf("c-1", "Main"));
  await openAdvanced();
  await click(groupPicker("Console sessions", "Model"));
  await choose("Beta");
  expect(groupPicker("Console sessions", "Model").textContent).toContain("Beta");
  elsewhere("Main", { "launch.claude.model": 3 }, { launch: { claude: { model: "alpha" } } });
  expect(groupPicker("Console sessions", "Model").textContent).toContain("Alpha");
  type("Name", "Renamed");
  await clickButton("Save");
  expect(sent(request)).toEqual([{ type: "update_console", console: "c-1", name: "Renamed", base_versions: { name: 1 } }]);
});

it("puts a conflicting field back to the current value and keeps the other edit, and a retry is based on the current version", async () => {
  const alerts = errorAlerts.getSnapshot().length;
  let refuse = true;
  const request = render(consoleOf("c-1", "Main"), [], (body: ClientRequestBody) => {
    if (body.type === "update_console" && refuse) return Promise.reject(conflict);
    return Promise.resolve({ type: "ack", id: null } as Event);
  });
  type("Name", "Mine");
  await click(picker("Console sessions"));
  await choose("Codex");
  await clickButton("Save");
  expect(errorAlerts.getSnapshot().length).toBe(alerts + 1);
  // The reply came before the other client's change: the form still shows what was typed.
  expect(inputLabelled("Name").value).toBe("Mine");
  // The broadcast then brings the console as the other client left it.
  act(() => lastStore.setState({ consoles: new Map([["c-1", consoleOf("c-1", "Theirs", { versions: { name: 8, default_agent: 1 } })]]) }));
  expect(inputLabelled("Name").value).toBe("Theirs");
  expect(picker("Console sessions").textContent).toContain("Codex");
  refuse = false;
  await clickButton("Save");
  // The name is the current one now, so it is not sent again; the other edit is, unchanged.
  expect(sent(request)[1]).toEqual({ type: "update_console", console: "c-1", default_agent: "codex", base_versions: { default_agent: 1 } });
  type("Name", "Mine again");
  await clickButton("Save");
  expect(sent(request)[2]).toMatchObject({ name: "Mine again", base_versions: { name: 8, default_agent: 1 } });
});
