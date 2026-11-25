// A provider's codes with a remedy of their own (`provider.<id>.<code>.remedy`), registered under the test-only provider
// `test_agent`, for the tests of what generic code does with such a remedy: no registered provider has a remedy that names
// a param of its code, and a generic test does not lean on a real agent's module where the test provider's will do. A test
// file mocks the registry with it:
//
//   vi.mock("../../providers/registry", async (importOriginal) => (await import("../../test/remedyProvider")).withRemedyProvider(await importOriginal()));
//
// Nothing else in the file's module graph changes: the registry's own values pass through, and the provider's presentation
// module gains `params`, `recoveryReferences`, `codes` and the messages, the catalogs its messages and brand placeholder, the code registry
// its records.
import type { CodeRecord } from "../errors/codes";
import { type Registry, withTestAgent } from "./testAgentRegistry";

/** The code whose remedy names a param of its code (`path`, a code param drawn as a reference), one the UI supplies
 * (`version`), one the daemon sends that its record does not list (`note`, shown as plain text), the module's own
 * reference (`command`), the provider (`agent`) and a shared recovery param (`appName`). Its failure carries a raw
 * diagnostic block in `detail`, beside the plain sentence. */
export const FIX_CODE = "provider.test_agent.fix_file";

/** The code whose remedy names `path` and `command` too, though its record lists `command` as a code param: the module's
 * own reference of that name wins over the code's. */
export const CLASH_CODE = "provider.test_agent.clash";

/** The brand name the test-only provider declares as a global placeholder (`params` of its module), by placeholder. */
export const TEST_BRAND_PARAMS = { testAgentBrand: "Test Brand" } as const;

/** The code whose remedy names global placeholders (`osName`, the provider's brand name), which the message fills itself: a param of
 * the same name that the failure carries is never used for them. */
export const GLOBAL_CODE = "provider.test_agent.global_named";

/** A code of the provider that has no remedy: actionable with nothing to do. */
export const NO_REMEDY_CODE = "provider.test_agent.no_remedy";

/** The `recoveryReferences` the provider declares under the test. */
export const REMEDY_REFERENCES = { command: "test-agent fix" } as const;

const MESSAGES = {
  [FIX_CODE]: "{agent} could not use {path}.",
  [`${FIX_CODE}.remedy`]: "Repair {path} for {agent} with {command}, check {version} and {note}, then retry in {appName}.",
  [CLASH_CODE]: "{agent} clashed over {path}.",
  [`${CLASH_CODE}.remedy`]: "Run {command} for {agent} in {appName}, as {path} says.",
  [GLOBAL_CODE]: "{agent} failed.",
  [`${GLOBAL_CODE}.remedy`]: "Check {osName} and {testAgentBrand} for {agent}.",
  [NO_REMEDY_CODE]: "{agent} refused.",
};

const CODES: Record<string, CodeRecord> = {
  [FIX_CODE]: { origin: "daemon", recovery: `${FIX_CODE}.remedy` as never, codeParams: ["path"], uiParams: { version: "9.9" }, detailBlock: true, summaryForDetail: false },
  [CLASH_CODE]: { origin: "daemon", recovery: `${CLASH_CODE}.remedy` as never, codeParams: ["command", "path"] },
  [GLOBAL_CODE]: { origin: "daemon", recovery: `${GLOBAL_CODE}.remedy` as never },
  [NO_REMEDY_CODE]: { origin: "daemon", causeOnly: true },
};

/** `registry` with the test-only provider's codes, remedies and references added. */
export function withRemedyProvider(registry: Registry): Registry {
  return withTestAgent(registry, { messages: MESSAGES, codes: CODES, params: TEST_BRAND_PARAMS, recoveryReferences: REMEDY_REFERENCES });
}
