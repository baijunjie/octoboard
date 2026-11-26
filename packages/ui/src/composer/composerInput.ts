import type { Session } from "../protocol";
import type { Daemon } from "../store";
import { sessionComposerEnabled } from "./composerSetting";

/** Whether a session's composer is offered: it is on in this window (`enabled`), the daemon keeps a queue (`offered`),
 * and the session is one a message can be written to (there, and not archived). */
export function composerOffered(enabled: boolean, offered: boolean, session: Pick<Session, "status"> | undefined): boolean {
  return enabled && offered && session !== undefined && session.status !== "archived";
}

/**
 * Whether text put into `session`'s input (a Quote, Add as context) goes into its composer rather than its terminal:
 * whether the composer is offered for it now, read when asked. The same test as `useComposerShown`, for an event
 * handler rather than a render.
 */
export function composerTakesInput(store: Daemon["store"], session: string): boolean {
  const state = store.getState();
  return composerOffered(sessionComposerEnabled.get(), state.messageQueues !== null, state.sessions.get(session));
}
