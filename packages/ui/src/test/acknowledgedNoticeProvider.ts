// A provider's notices marked as ones to acknowledge (`acknowledgedNotice` in `errors/codes.ts`), registered under the
// test-only provider `test_agent`, for the tests of what generic code does with the mark: a generic test does not lean
// on a real agent's module where the test provider's will do. A test file mocks the registry with it:
//
//   vi.mock("../providers/registry", async (importOriginal) => (await import("../test/acknowledgedNoticeProvider")).withAcknowledgedNotices(await importOriginal()));
//
// Nothing else in the file's module graph changes: the registry's own values pass through, and the provider's
// presentation module gains `codes` and the messages, the catalogs the messages, the code registry its records.
import type { CodeRecord } from "../errors/codes";
import { type Registry, withTestAgent } from "./testAgentRegistry";

/** A launch notice to acknowledge, with nothing to do about it and no title of its own. */
export const SETTINGS_NOTICE = "provider.test_agent.settings_not_read";

/** A second launch notice to acknowledge, of the same launch as the first, which carries a remedy of its own. */
export const LEFTOVERS_NOTICE = "provider.test_agent.leftovers_not_read";

/** A launch notice the provider does not mark: a toast, as every notice was before the mark existed. */
export const PLAIN_NOTICE = "provider.test_agent.hooks_off";

const MESSAGES = {
  [SETTINGS_NOTICE]: "{agent} did not read {path}, so the permission rules in it are not in force for this launch.",
  [LEFTOVERS_NOTICE]: "{agent} did not read the rules an earlier version left in {path}.",
  [`${LEFTOVERS_NOTICE}.remedy`]: "Move the rules out of {path} for {agent} to read them.",
  [PLAIN_NOTICE]: "{agent} runs with its hooks off.",
};

const CODES: Record<string, CodeRecord> = {
  [SETTINGS_NOTICE]: { origin: "daemon", acknowledgedNotice: true, codeParams: ["path"] },
  [LEFTOVERS_NOTICE]: { origin: "daemon", acknowledgedNotice: true, recovery: `${LEFTOVERS_NOTICE}.remedy` as never, codeParams: ["path"] },
  [PLAIN_NOTICE]: { origin: "daemon" },
};

/** `registry` with the test-only provider's notices added. */
export function withAcknowledgedNotices(registry: Registry): Registry {
  return withTestAgent(registry, { messages: MESSAGES, codes: CODES });
}
