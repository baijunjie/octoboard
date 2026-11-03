// The daemon stand-in the tests render under. Every member does nothing, so a test names only what it
// reads, and a member `Daemon` gains later is added here once.
import { createStateStore, type Daemon, type State } from "../store";

/** A `Daemon` over a store that starts at `state`, whose requests are rejected unless the test gives
 * its own `request`, and whose other members do nothing. `overrides` replace members as they are. */
export function testDaemon({ state, request = () => Promise.reject(new Error("no daemon")), ...overrides }: { state?: Partial<State> } & Partial<Daemon> = {}): Daemon {
  return {
    store: createStateStore(state),
    request,
    raiseError: () => {},
    toastNotice: () => {},
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
    ...overrides,
  };
}
