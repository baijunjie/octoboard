import { describe, expect, it, vi } from "vitest";

import { CodedError } from "../errors/codedError";
import { DEFAULT_SETTINGS } from "./defaultSettings";
import { createSettingsWriter } from "./settingsWrite";
import { createStateStore } from "./state";

function setup() {
  const store = createStateStore({ settings: { ...DEFAULT_SETTINGS, versions: { daemon_update_check: 3, daemon_update_channel: 4 } } });
  const sent: unknown[] = [];
  const answers: { fail?: unknown } = {};
  const request = vi.fn(async (body: unknown) => {
    sent.push(body);
    if (answers.fail) throw answers.fail;
    return { type: "ack", id: null };
  });
  const failures: unknown[] = [];
  const write = createSettingsWriter({ store, request: request as never, onFailure: (error) => failures.push(error) });
  const broadcast = (versions: Record<string, number>) => store.setState((state) => ({ settings: { ...state.settings, versions: { ...state.settings.versions, ...versions } } }));
  return { write, sent, answers, failures, broadcast };
}

describe("createSettingsWriter", () => {
  it("sends the changes based on the version the store holds of each path, leaving unversioned paths unchecked", async () => {
    const { write, sent } = setup();
    await write({ daemon_update_check: false }, ["daemon_update_check"]);
    await write({ interface_language: "de" }, ["interface_language"]);
    expect(sent).toEqual([
      { type: "update_settings", daemon_update_check: false, base_versions: { daemon_update_check: 3 } },
      { type: "update_settings", interface_language: "de", base_versions: undefined },
    ]);
  });

  it("drops a write of a path while the store still holds the version the last one was based on", async () => {
    const { write, sent, broadcast } = setup();
    await write({ daemon_update_check: false }, ["daemon_update_check"]);
    await write({ daemon_update_check: true }, ["daemon_update_check"]);
    expect(sent).toHaveLength(1);
    broadcast({ daemon_update_check: 4 });
    await write({ daemon_update_check: true }, ["daemon_update_check"]);
    expect(sent).toHaveLength(2);
    expect(sent[1]).toMatchObject({ base_versions: { daemon_update_check: 4 } });
  });

  it("hands a failure on, frees the path for another try, and settles", async () => {
    const { write, sent, answers, failures } = setup();
    answers.fail = new CodedError("refused", "write_conflict", {});
    await write({ daemon_update_check: false }, ["daemon_update_check"]);
    expect(failures).toHaveLength(1);
    answers.fail = undefined;
    await write({ daemon_update_check: false }, ["daemon_update_check"]);
    expect(sent).toHaveLength(2);
  });
});
