// The wording of two login questions, registered under the test-only provider `test_agent`, for the tests of what the
// login dialog does with a question a provider words: GitHub Copilot CLI words one of its own, but a generic test does
// not lean on a real agent's module, and these two cover what its one question does not (a second question, a question
// with no params). A test file mocks the registry with it:
//
//   vi.mock("../../providers/registry", async (importOriginal) => (await import("../../test/loginQuestionProvider")).withLoginQuestionProvider(await importOriginal()));
//
// Nothing else in the file's module graph changes: the registry's own values pass through, and the provider's module
// and the catalogs gain the questions' messages.
import { type Registry, withTestAgent } from "./testAgentRegistry";

/** A question whose wording names a param of its own (`path`, drawn as a literal). */
export const KEEP_TOKEN = "keep_token";

/** A second question, with no params, for a login that moves from one question to another. */
export const SHARE_USAGE = "share_usage";

const MESSAGES = {
  [`provider.test_agent.loginQuestion.${KEEP_TOKEN}`]: "Keep the token in the file {path}?",
  [`provider.test_agent.loginQuestion.${KEEP_TOKEN}.yes`]: "Keep in the file",
  [`provider.test_agent.loginQuestion.${KEEP_TOKEN}.no`]: "Do not keep",
  [`provider.test_agent.loginQuestion.${SHARE_USAGE}`]: "Share usage data with the test agent's maker?",
  [`provider.test_agent.loginQuestion.${SHARE_USAGE}.yes`]: "Share usage data",
  [`provider.test_agent.loginQuestion.${SHARE_USAGE}.no`]: "Do not share",
};

/** `registry` with the wording of the test-only provider's login questions added, in every language. */
export function withLoginQuestionProvider(registry: Registry): Registry {
  return withTestAgent(registry, { messages: MESSAGES });
}
