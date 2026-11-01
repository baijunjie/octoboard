import { useCallback, useEffect, useRef, useState, type RefObject } from "react";

import { useDaemonStore } from "../store";
import type { TerminalController, TermStatus } from "./TerminalController";
import { MAX_AUTO_RECONNECT_ATTEMPTS, reconnectDelay, STABLE_CONNECTION_MS } from "./reconnectPolicy";

/**
 * What a terminal view does about its socket dropping, for the target (a session, a shell) it is
 * attached to: the status the controller reports, automatic reconnects with backoff, the recovery
 * a fresh snapshot gives a terminal that gave up, and the `trouble` that covers the terminal.
 *
 * Wire `onStatusChange` into `useTerminalController`. Attaching is the caller's (`attach`, with
 * whether the user asked for it); `resetAttempts` and `attemptsSpent` are for a caller that has to
 * clear or check the count where it attaches or detaches.
 */
export function useTerminalReconnect({
  controllerRef,
  initialStatus,
  targetId,
  live,
  attach,
  terminalReconnectDelay,
}: {
  controllerRef: RefObject<TerminalController | undefined>;
  /** What the status is before the controller has reported: `connecting` for a view that attaches as it mounts. */
  initialStatus: TermStatus;
  /** What the controller is attached to, or `undefined` while there is nothing. */
  targetId: string | undefined;
  /** Whether the target's socket should be up at all, per its own record rather than the controller's last
   * outcome: a socket that closed on a live target is a drop, on one that has ended it is the end. */
  live: boolean;
  attach: (userInitiated: boolean) => void;
  terminalReconnectDelay?: (attempt: number) => number;
}): {
  status: TermStatus;
  onStatusChange: (status: TermStatus) => void;
  resetAttempts: () => void;
  /** Whether the automatic attempts are spent: only the user's Reconnect, or a fresh snapshot, retries then. */
  attemptsSpent: () => boolean;
  trouble: "reconnecting" | "disconnected" | undefined;
  canReconnect: boolean;
  reconnectNow: () => void;
} {
  const [status, setStatus] = useState<TermStatus>(initialStatus);
  const attemptsRef = useRef(0);
  const stableTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);

  const onStatusChange = useCallback((next: TermStatus) => {
    if (stableTimerRef.current) {
      clearTimeout(stableTimerRef.current);
      stableTimerRef.current = undefined;
    }
    if (next === "open") {
      stableTimerRef.current = setTimeout(() => {
        attemptsRef.current = 0;
      }, STABLE_CONNECTION_MS);
    }
    setStatus(next);
  }, []);
  useEffect(
    () => () => {
      if (stableTimerRef.current) clearTimeout(stableTimerRef.current);
    },
    [],
  );

  const resetAttempts = useCallback(() => {
    attemptsRef.current = 0;
  }, []);
  const attemptsSpent = useCallback(() => attemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS, []);
  useEffect(() => {
    resetAttempts();
  }, [targetId]);

  // A fresh snapshot (the control connection came back, possibly after an outage longer than the
  // automatic attempts' budget) is a deliberate chance to recover a terminal that gave up: the
  // caller's own attach skips ordinary updates once the attempts are spent. Not user-initiated, so
  // it does not pull focus.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || targetId === undefined || !live) return;
    if (controller.currentSessionId !== targetId) return;
    if (controller.currentStatus !== "closed" && controller.currentStatus !== "not_running") return;
    if (attemptsRef.current < MAX_AUTO_RECONNECT_ATTEMPTS) return;
    attemptsRef.current = 0;
    attach(false);
    // Keyed on the epoch alone: the target's own changes are the caller's business.
  }, [snapshotEpoch]);

  // Reconnects a live target after its socket landed on `closed` or `not_running` — the daemon
  // drops a client that stops draining for a few seconds (an occluded window, a large output burst)
  // and documents that the client is expected to reconnect and get the ring buffer replayed, not
  // treat the drop as the target having ended; and a reattach that itself fails to connect
  // (`not_running`) needs the same retry, not a dead end. Never user-initiated: a background heal
  // must not pull focus out of whatever the user is typing into.
  useEffect(() => {
    const controller = controllerRef.current;
    if (!controller || targetId === undefined || !live) return;
    if (status !== "closed" && status !== "not_running") return;
    if (controller.currentSessionId !== targetId) return;
    if (attemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS) return;
    const delay = reconnectDelay(attemptsRef.current, terminalReconnectDelay);
    const timer = setTimeout(() => {
      attemptsRef.current += 1;
      attach(false);
    }, delay);
    return () => clearTimeout(timer);
    // Deliberately keyed on the target's id rather than its record: an update arriving for this
    // target would otherwise tear the pending timer down and start it over, so under a working
    // agent the reconnect could be postponed indefinitely.
  }, [status, targetId, live, attach, terminalReconnectDelay]);

  // Works out what is wrong with the connection: `disconnected` once the attempts are spent. The
  // controller's own status is read rather than `status`: a switch of target attaches in an effect,
  // so the state still holds the previous target's outcome for one render, while the controller
  // already has the new one's. A first connect that has not failed yet is not trouble (typing
  // meanwhile is queued); once an attempt has failed, `connecting` is the reconnect cycle's middle
  // and stays `reconnecting`. Stored with the target it was worked out for, so that the first render
  // after a switch does not show the previous target's.
  const [troubleOf, setTroubleOf] = useState<{ target: string; state: "reconnecting" | "disconnected" }>();
  useEffect(() => {
    const controller = controllerRef.current;
    let next: "reconnecting" | "disconnected" | undefined;
    if (controller && targetId !== undefined && live && controller.currentSessionId === targetId) {
      const current = controller.currentStatus;
      const failed = current === "closed" || current === "not_running";
      if (current === "open" || (current === "connecting" && attemptsRef.current === 0)) next = undefined;
      else if (failed && attemptsRef.current >= MAX_AUTO_RECONNECT_ATTEMPTS) next = "disconnected";
      else next = "reconnecting";
    }
    setTroubleOf((prev) => {
      if (targetId === undefined || !next) return undefined;
      return prev?.target === targetId && prev.state === next ? prev : { target: targetId, state: next };
    });
  }, [status, targetId, live]);
  const trouble = troubleOf?.target === targetId ? troubleOf?.state : undefined;

  // While the daemon connection is down the connection banner owns recovery and its Retry; the
  // terminal's own Reconnect is not offered then.
  const canReconnect = trouble === "disconnected" && connectionState === "open";
  const reconnectNow = useCallback(() => {
    attemptsRef.current = 0;
    attach(true);
  }, [attach]);

  return { status, onStatusChange, resetAttempts, attemptsSpent, trouble, canReconnect, reconnectNow };
}
