// @vitest-environment jsdom
import { readdirSync, readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { describe, expect, it } from "vitest";

import { CodedError } from "./errors/codedError";
import { CODES, codeRecord, type CodeRecord, type MessageFacets } from "./errors/codes";
import { messageCode, RECORD_NAMES } from "./errors/daemonMessage";
import { messageKey } from "./errors/codes";
import { noticeKind } from "./errors/errorPresentation";
import { classifyError } from "./errors/errorRecovery";
import { en } from "./i18n/messages/en";
import { zhHans } from "./i18n/messages/zh-Hans";

// Every code the daemon can send is worded in the catalogs, and every failure it can answer a request with, or notify
// as one, has a remedy (`recovery` of its record in `errors/codes.ts`): otherwise the window shows the daemon's English
// text, or calls a deliberate refusal a bug and asks for an Issue. The codes of a quota reading, a models list and a
// login are shown where they are read, not in an alert classified by code, so they need only their words.
//
// The daemon's registry of the codes it sends: a code is a `Code`, which only `protocol/codes.rs` can make, and each is
// a `pub const NAME: Code = Code("code");` of one of its code modules.
const MODULES = ["error_code", "models_unavailable_code", "quota_unavailable_code", "login_failure_code", "agent_cli_code", "notice_code"] as const;
type Module = (typeof MODULES)[number];

function readRegistry(): Record<Module, string[]> {
  const source = readFileSync(new URL("../../../apps/daemon/src/protocol/codes.rs", pathToFileURL(`${import.meta.dirname}/`)), "utf8");
  const registry = {} as Record<Module, string[]>;
  let declared = 0;
  for (const module of MODULES) {
    const body = new RegExp(`\\npub mod ${module} \\{\\n([\\s\\S]*?)\\n\\}\\n`).exec(source)?.[1];
    if (body === undefined) throw new Error(`protocol/codes.rs has no module ${module}`);
    registry[module] = [...body.matchAll(/pub const [A-Z0-9_]+: Code =\s*Code\("([a-z0-9_]+)"\);/g)].map((match) => match[1]);
    declared += registry[module].length;
  }
  // A code made in any other shape would escape this reading.
  const made = source.match(/\bCode\("/g)?.length ?? 0;
  if (made !== declared) throw new Error(`protocol/codes.rs makes ${made} codes, of which ${declared} are read as declarations`);
  return registry;
}

// The codes of the providers: each provider's file under `providers/` declares its own, as
// `pub const NAME: ProviderCode = ProviderCode::<kind>("provider.<id>.<code>");` (rustfmt may wrap after the `=`), where
// the kind is an `error` or a `notice`.
function readProviderCodes(): Pick<Record<Module, string[]>, "error_code" | "notice_code"> {
  const directory = new URL("../../../apps/daemon/src/providers/", pathToFileURL(`${import.meta.dirname}/`));
  const found = { error_code: [] as string[], notice_code: [] as string[] };
  for (const file of readdirSync(directory).filter((name) => name.endsWith(".rs") && name !== "mod.rs")) {
    const source = readFileSync(new URL(file, directory), "utf8");
    const declared = [...source.matchAll(/pub const [A-Z0-9_]+: ProviderCode =\s*ProviderCode::(error|notice)\("(provider\.[a-z0-9_]+\.[a-z0-9_]+)"\);/g)];
    for (const [, kind, code] of declared) {
      expect(code, `${file} declares ${code}`).toMatch(new RegExp(`^provider\\.${file.slice(0, -3)}\\.`));
      found[kind === "error" ? "error_code" : "notice_code"].push(code);
    }
    // A code made in any other shape would escape this reading.
    const made = source.match(/\bProviderCode::(?:error|notice)\("/g)?.length ?? 0;
    if (made !== declared.length) throw new Error(`providers/${file} makes ${made} codes, of which ${declared.length} are read as declarations`);
  }
  return found;
}

const CORE = readRegistry();
const PROVIDER = readProviderCodes();
const REGISTRY = { ...CORE, error_code: [...CORE.error_code, ...PROVIDER.error_code], notice_code: [...CORE.notice_code, ...PROVIDER.notice_code] };

const ERROR_CODES = REGISTRY.error_code;
const FAILURE_NOTICES = REGISTRY.notice_code.filter((code) => noticeKind(code, {}) === "error");
const ALL_CODES = [...new Set(MODULES.flatMap((module) => REGISTRY[module]))];

it("reads the daemon's registry", () => {
  for (const module of MODULES) expect(REGISTRY[module].length, module).toBeGreaterThan(0);
  expect(ERROR_CODES).toContain("internal_error");
  expect(REGISTRY.notice_code).toContain("launch_failed");
  expect(PROVIDER.error_code.length).toBeGreaterThan(0);
  expect(PROVIDER.notice_code.length).toBeGreaterThan(0);
  for (const code of [...PROVIDER.error_code, ...PROVIDER.notice_code]) expect(code, code).toMatch(/^provider\.[a-z0-9_]+\.[a-z0-9_]+$/);
});

describe.each(ALL_CODES)("the daemon's code %s", (code) => {
  it("is worded in English and Simplified Chinese", () => {
    const key = messageKey(messageCode(code, {}));
    expect(Object.hasOwn(en, key), `en lacks ${key}`).toBe(true);
    expect(Object.hasOwn(zhHans, key), `zh-Hans lacks ${key}`).toBe(true);
  });
});

const records = Object.entries(CODES) as Array<[string, CodeRecord]>;
/** A code's messages with the facets of their params: its own, and each variant `messageCode` words it under. */
const messagesOf = (code: string, record: CodeRecord): Array<[string, MessageFacets]> =>
  [[code, record], ...Object.entries(record.variants ?? {}).map(([variant, facets]): [string, MessageFacets] => [`${code}.${variant}`, facets])];
/** The messages a code is worded under: those its record lists, and those `messageCode` picks for its params, which a
 * record lists only where the params differ (the `cause` of a refused directory, say). */
const wordedUnder = (code: string, record: CodeRecord): string[] => {
  const probes = [{}, ...["unanswered", "unreadable", "other_unlooked", "signing_in"].map((cause) => ({ cause }))];
  return [...new Set([...messagesOf(code, record).map(([message]) => message), ...probes.map((params) => messageCode(code, params))])];
};
/** The catalog's English texts a message's params are shown in: its sentence, its opening sentence and its remedy line;
 * a trust failure's `reason` is worded from `daemon.trust_reason.<reason_code>`, filled in with the same params. */
const textsOf = (code: string, message: string): string => {
  const key = messageKey(message);
  const reasons = code === "trust_answer_failed" || code === "trust_not_carried_over"
    ? Object.keys(en).filter((k) => k.startsWith("daemon.trust_reason."))
    : [];
  return [key, `${key}.summary`, `${key}.recovery`, ...reasons]
    .map((k) => (Object.hasOwn(en, k) ? JSON.stringify(en[k as keyof typeof en]) : "")).join(" ");
};
const has = (key: string) => Object.hasOwn(en, key) && Object.hasOwn(zhHans, key);

describe("the registry in errors/codes.ts", () => {
  it("has a daemon record for every code the daemon declares, and for no other", () => {
    expect(records.filter(([, record]) => record.origin === "daemon").map(([code]) => code).sort()).toEqual([...ALL_CODES].sort());
  });

  it("holds the codes of client and UI origin apart from the daemon's", () => {
    for (const [code, record] of records) if (record.origin !== "daemon") expect(ALL_CODES, code).not.toContain(code);
  });

  it("marks a failure notice only on a code the daemon sends as a notice, and words its title", () => {
    for (const [code, record] of records) {
      if (!record.failureNotice) continue;
      expect(REGISTRY.notice_code, code).toContain(code);
      expect(record.alertTitle, code).toBe(true);
    }
  });

  it("marks an alert title exactly where the catalogs have one", () => {
    for (const [code, record] of records) {
      expect(has(`${messageKey(code)}.alertTitle`), code).toBe(record.alertTitle === true);
    }
  });

  it("words every variant", () => {
    for (const [code, record] of records) {
      for (const variant of Object.keys(record.variants ?? {})) expect(has(`${messageKey(code)}.${variant}`), `${code}.${variant}`).toBe(true);
    }
  });

  it("shows a param as a chip only where the sentence has it", () => {
    for (const [code, record] of records) {
      for (const [message, facets] of messagesOf(code, record)) {
        const text = textsOf(code, message);
        for (const name of [...(facets.codeParams ?? []), ...(facets.nameParams ?? [])]) expect(text, `${message} {${name}}`).toContain(`{${name}}`);
      }
    }
  });

  it("names the params its other facets refer to", () => {
    for (const [code, record] of records) {
      const messages = messagesOf(code, record);
      const named = new Set(messages.flatMap(([, facets]) => facets.nameParams ?? []));
      // A record id and a default-account name are shown as a name; `owner` is a session and `page` has no record to name.
      for (const name of record.recordIds ?? []) {
        expect(named, `${code} recordIds ${name}`).toContain(name);
        expect([...Object.keys(RECORD_NAMES), "owner", "page"], `${code} recordIds ${name}`).toContain(name);
      }
      for (const name of record.defaultAccountParams ?? []) expect(named, `${code} defaultAccountParams ${name}`).toContain(name);
      for (const name of Object.keys(record.uiParams ?? {})) {
        const mentioned = wordedUnder(code, record).some((message) => textsOf(code, message).includes(`{${name}}`));
        expect(mentioned, `${code} uiParams ${name}`).toBe(true);
      }
    }
  });

  it("gives a code with no opening sentence none, and a rich sentence to the rest that need one", () => {
    // The opening sentence of a message is its `.summary`; a variant of a code with a detail block has one only where
    // the detail can be there, so it is the code's messages together that must have it.
    for (const [code, record] of records) {
      const worded = wordedUnder(code, record).filter((message) => has(`${messageKey(message)}`));
      const some = (part: string) => worded.some((message) => has(`${messageKey(message)}.${part}`));
      if (record.noSummary) expect(some("summary"), code).toBe(false);
      else if (record.recoveryLine || (record.detailBlock && record.summaryForDetail !== false)) expect(some("summary"), code).toBe(true);
      if (record.recoveryLine) expect(some("recovery"), code).toBe(true);
    }
  });
});

const CLASSIFIED = [...new Set([...ERROR_CODES, ...FAILURE_NOTICES])];

describe.each(CLASSIFIED)("the daemon's failure %s", (code) => {
  it("is classified with a remedy, or is unexpected for a reason the registry gives", () => {
    const record = codeRecord(code)!;
    expect(record.recovery !== undefined || record.causeOnly === true || record.unexpected !== undefined, "recovery, causeOnly or unexpected").toBe(true);
    expect(classifyError(new CodedError("text", code, {})).kind).toBe(record.unexpected === undefined ? "actionable" : "unexpected");
  });
});

it("gives a reason only to a code the daemon sends as a failure", () => {
  for (const [code, record] of records) if (record.unexpected !== undefined) expect(CLASSIFIED, code).toContain(code);
});
