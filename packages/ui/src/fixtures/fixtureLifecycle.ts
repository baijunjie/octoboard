// What the fixture daemon answers for the lifecycle of the daemon itself: which running work a restart into an update
// would not carry over, and how it takes a check or an install of its own update. `shutdown` and `planned_stop` need no
// module: they are acknowledged.
import { isLive } from "../sessions/sessionStatus";
import { refuseUnfillable } from "./banner";
import { FIXTURE_DAEMON_INSTALL_STARTED_AT } from "./builders";
import type { FixtureContext, Handlers } from "./fixtureContext";

/** The replies about the daemon's own lifecycle. */
export function createLifecycleReplies({ scenario, store }: FixtureContext): Handlers {
  return {
    // The daemon starts a check, after the last one that ended at the `at` its status holds; a scenario ends it by sending a
    // status whose last check has another `at`.
    check_daemon_update: () => ({ type: "daemon_update_check", id: null, started: true, last_check_at: store.getState().daemonUpdate?.last_check?.at ?? null }),
    // The install a scenario ends is the one with `started_at` as named here (`FIXTURE_DAEMON_INSTALL_STARTED_AT`).
    update_daemon: () => ({ type: "daemon_update_started", id: null, started_at: FIXTURE_DAEMON_INSTALL_STARTED_AT }),
    handover_check: () => {
      const { known = true, sessions = [], unnamedSessions = [], shellTerminals = [], unreachedShellTerminals = [] } = scenario.handover ?? {};
      // The daemon lists only what runs: a session without a process or an exited shell has nothing to interrupt.
      const state = store.getState();
      for (const id of sessions) {
        const session = state.sessions.get(id);
        if (!session || !isLive(session.status)) refuseUnfillable(`handover_check lists session ${id}, which is not a running session of the scenario`);
      }
      // An id the window can name is a mistake of the scenario, since it would then be named.
      for (const id of unnamedSessions) {
        if (state.sessions.has(id)) refuseUnfillable(`handover_check lists session ${id} as one the window does not know, but the scenario holds it`);
      }
      for (const id of shellTerminals) {
        if (state.shellTerminals.get(id)?.state !== "running") refuseUnfillable(`handover_check lists shell terminal ${id}, which is not a running shell terminal of the scenario`);
      }
      return { type: "handover_check", id: null, known, sessions: [...sessions, ...unnamedSessions], shell_terminals: shellTerminals, unreached_shell_terminals: unreachedShellTerminals };
    },
  };
}
