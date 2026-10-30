// @vitest-environment jsdom
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { describe, expect, it } from "vitest";

import { DaemonRequestError } from "./daemon-client";
import { messageCode } from "./daemonMessage";
import { noticeKind } from "./errorPresentation";
import { classifyError } from "./errorRecovery";
import { en } from "./i18n/messages/en";
import { zhHans } from "./i18n/messages/zh-Hans";

// Every code the daemon can send is worded in the catalogs, and every failure it can answer a request with, or notify
// as one, has a remedy (`GROUPS` in `errorRecovery.ts`): otherwise the window shows the daemon's English text, or
// calls a deliberate refusal a bug and asks for an Issue. The codes of a quota reading, a models list and a login are
// shown where they are read, not in an alert classified by code, so they need only their words.
//
// The daemon's registry of the codes it sends: a code is a `Code`, which only `protocol.rs` can make, and each is a
// `pub const NAME: Code = Code("code");` of one of its code modules.
const MODULES = ["error_code", "models_unavailable_code", "quota_unavailable_code", "login_failure_code", "notice_code"] as const;
type Module = (typeof MODULES)[number];

function readRegistry(): Record<Module, string[]> {
  const source = readFileSync(new URL("../../../apps/daemon/src/protocol.rs", pathToFileURL(`${import.meta.dirname}/`)), "utf8");
  const registry = {} as Record<Module, string[]>;
  let declared = 0;
  for (const module of MODULES) {
    const body = new RegExp(`\\npub mod ${module} \\{\\n([\\s\\S]*?)\\n\\}\\n`).exec(source)?.[1];
    if (body === undefined) throw new Error(`protocol.rs has no module ${module}`);
    registry[module] = [...body.matchAll(/pub const [A-Z0-9_]+: Code =\s*Code\("([a-z0-9_]+)"\);/g)].map((match) => match[1]);
    declared += registry[module].length;
  }
  // A code made in any other shape would escape this reading.
  const made = source.match(/\bCode\("/g)?.length ?? 0;
  if (made !== declared) throw new Error(`protocol.rs makes ${made} codes, of which ${declared} are read as declarations`);
  return registry;
}

const REGISTRY = readRegistry();

// Codes the window shows as unexpected on purpose, or classifies by another code they carry, each with why.
const NOT_CLASSIFIED_BY_CODE: Record<string, string> = {
  internal_error: "a failure the daemon did not foresee, which is what the unexpected alert is for",
  relocation_failed: "classified by its `cause`; one the window has no remedy for is unexpected",
  launch_failed: "shown as the failure its `reason_code` names",
  account_switch_automatically_failed: "classified by the failure its `reason_code` names",
  // Refusals of requests the window never makes, so meeting one is the window's own bug.
  unreadable_request: "the window sends only requests of the shapes it knows",
  conflicting_fields: "the window never sends two fields that exclude each other",
  field_required: "the window always sends the fields a request needs",
  field_too_long: "the forms that send a project's name and description hold them to the daemon's limits",
  invalid_commit: "the window names only commits the daemon gave it",
  invalid_change: "the window names only sides the change's group can read",
  console_session_trust_not_asked: "the window never offers to answer a console session's trust confirmation",
  handoff_to_same_agent: "the handoff dialog never offers the session's own agent",
  resume_for_console_session: "the window never asks to resume a conversation for a console session",
  request_superseded: "a browse request given up for a newer one, which the window shows nothing for",
};

const ERROR_CODES = REGISTRY.error_code;
const FAILURE_NOTICES = REGISTRY.notice_code.filter((code) => noticeKind(code, {}) === "error");
const ALL_CODES = [...new Set(MODULES.flatMap((module) => REGISTRY[module]))];

it("reads the daemon's registry", () => {
  for (const module of MODULES) expect(REGISTRY[module].length, module).toBeGreaterThan(0);
  expect(ERROR_CODES).toContain("internal_error");
  expect(REGISTRY.notice_code).toContain("launch_failed");
});

describe.each(ALL_CODES)("the daemon's code %s", (code) => {
  it("is worded in English and Simplified Chinese", () => {
    const key = `daemon.${messageCode(code, {})}`;
    expect(Object.hasOwn(en, key), `en lacks ${key}`).toBe(true);
    expect(Object.hasOwn(zhHans, key), `zh-Hans lacks ${key}`).toBe(true);
  });
});

describe.each([...new Set([...ERROR_CODES, ...FAILURE_NOTICES])].filter((code) => !(code in NOT_CLASSIFIED_BY_CODE)))(
  "the daemon's failure %s",
  (code) => {
    it("is classified with a remedy rather than as unexpected", () => {
      expect(classifyError(new DaemonRequestError("text", code, {}))).toMatchObject({ kind: "actionable" });
    });
  },
);

it("exempts only codes the daemon sends as failures and the window does not classify by code", () => {
  for (const code of Object.keys(NOT_CLASSIFIED_BY_CODE)) {
    expect([...ERROR_CODES, ...FAILURE_NOTICES]).toContain(code);
    expect(classifyError(new DaemonRequestError("text", code, {})), code).toEqual({ kind: "unexpected" });
  }
});
