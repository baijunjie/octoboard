// A note about the sessions of the test-only provider `test_agent` (`sessionNote` of a presentation module), with the
// command it names declared as a reference, for the tests of what the agent's settings dialog does with such a note: a
// generic test does not lean on a real agent's module where the test provider's will do. A test file mocks the registry
// with it:
//
//   vi.mock("../../providers/registry", async (importOriginal) => (await import("../../test/sessionNoteProvider")).withSessionNote(await importOriginal()));
//
// Nothing else in the file's module graph changes: the registry's own values pass through, and the provider's
// presentation module gains the note and its reference, the catalogs the note.
import { type Registry, withTestAgent } from "./testAgentRegistry";

/** The command the note names, set apart as a left-to-right literal. */
export const SESSION_NOTE_COMMAND = "test-agent config";

const MESSAGES = {
  "provider.test_agent.sessionNote": "A session of {agent} forgets what it changed in its own configuration when it ends. To keep a change, run {command}.",
};

/** `registry` with the test-only provider's note about its sessions added, in every language. */
export function withSessionNote(registry: Registry): Registry {
  return withTestAgent(registry, { messages: MESSAGES, references: { sessionNote: { command: SESSION_NOTE_COMMAND } } });
}
