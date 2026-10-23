// @vitest-environment jsdom
import { expect, it } from "vitest";

import { DaemonRequestError } from "./daemon-client";
import { failureSummaryText } from "./errorText";
import { format } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import type { Console, Project, Session } from "./protocol";

const records = { consoles: new Map<string, Console>(), projects: new Map<string, Project>(), sessions: new Map<string, Session>() };
const t: Parameters<typeof failureSummaryText>[0] = (key, ...args) => format("en", key, args[0]);

it.each([
  ["an uncatalogued failure is the generic sentence", new DaemonRequestError("daemon words", "models_something_new", {}), "Something unexpected went wrong."],
  ["a thrown unexpected error is the generic sentence", new DaemonRequestError("boom", "unexpected_error", {}), "Something unexpected went wrong."],
  ["a plain string is itself", "Plain words", "Plain words"],
  [
    "a shell timeout is its cause",
    new DaemonRequestError("raw", "shell_environment_timeout", { shell: "/bin/zsh", command: "env -0", timeout: "10s" }),
    t("daemon.shell_environment_timeout.cause"),
  ],
  [
    "a catalogued code with params is its summary, without detail or remedy",
    new DaemonRequestError("raw", "models_binary_not_found", { binary: "claude" }),
    `${isolate("claude")} was not found, so the models cannot be listed.`,
  ],
  [
    "a detail of a catalogued code is left out",
    new DaemonRequestError("raw", "models_command_failed", { agent: "Codex", detail: "exit 1" }),
    `${isolate("Codex")} could not list its models.`,
  ],
  ["a catalogued code without its params is the message", new DaemonRequestError("Worded already", "models_binary_not_found", {}), "Worded already"],
])("%s", (_name, error, expected) => {
  expect(failureSummaryText(t, "en", error, records)).toBe(expected);
});
