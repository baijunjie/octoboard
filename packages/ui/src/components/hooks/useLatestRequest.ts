import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { browseFailure, CHANGED_RETRIES, projectBrowseFailure, type BrowseFailure } from "../../browse/failure";
import type { BrowseLane } from "../../browse/lane";
import type { Translate } from "../../i18n/catalog";
import { useCurrentLanguage, useT } from "../../i18n/react";
import { useDaemon, useDaemonStore } from "../../store";
import { useLatest } from "./useLatest";

/** A failure the caller shows: one the daemon gave, or what was read changing on every try. */
export type ShownFailure = Extract<BrowseFailure, { kind: "failed" | "changed" }>;

/** One request, and what its outcomes make of the value held. */
export interface LatestRequestSend<V, R> {
  /** The scope the answer is held for: the scope of the latest committed render when the property is left out (given as
   * `undefined`, it is the scope `undefined`). Given where the request is sent before the
   * render that names its scope is committed (a viewer opening a file, say). */
  scope?: string | undefined;
  /** Sends the request: when the `lane` gives it its turn, at once without one, and again for each retry. */
  request: () => Promise<R>;
  /** The window's lane the request waits in (`browse/budget.ts`); none for a request in a slot of its own. */
  lane?: BrowseLane;
  /** The slot the request claims, for a lane that lets a request replacing another in its slot go past it. */
  slot?: string;
  /** What is held while the request is out: unchanged when left out. */
  pending?: (held: V) => V;
  /** The change the reply makes to what is held, or nothing when the reply does not answer this request (another
   * shape, project or subject). Returning `held` itself publishes nothing. */
  accept: (reply: R) => ((held: V) => V) | undefined;
  /** What a failure makes of what is held, worded with `t` of the render current when it happened. */
  failed: (failure: ShownFailure, held: V, t: Translate) => V;
  /** What the connection closing while the request was out makes of what is held: unchanged when left out. */
  disconnected?: (held: V, t: Translate) => V;
  /** Whether the answer is still wanted, besides being the latest request's for the current scope. */
  wanted?: () => boolean;
  /** A rejection the caller handles itself before it is classified: returns true when it did. */
  handled?: (err: unknown) => boolean;
}

export interface LatestRequestOptions<V> {
  /** What the value shown belongs to: one held for another scope is never returned, `initial` is in its place. */
  scope: string | undefined;
  /** What is shown for a scope nothing has been held for yet. */
  initial: V;
  /** The project the requests are about, whose directory words a failure naming the project's own path. */
  project?: string;
  /** Called with the latest render's closure when the connection is back (a new `snapshotEpoch`): the daemon keeps
   * nothing from an earlier connection. `asked` tells whether any request was sent, `lost` whether the latest one
   * was lost with the connection. */
  onReconnect?: (last: { asked: boolean; lost: boolean }) => void;
}

export interface LatestRequest<V> {
  /** What is held for the current scope, or `initial`. */
  value: V;
  /** Sends a request, giving up the one before it: only the latest request's answer is taken, and only while the
   * caller is mounted and its scope is the one asked for. Settles once the answer is taken or has failed, or when a
   * newer request has taken its place. */
  send: <R>(request: LatestRequestSend<V, R>) => Promise<void>;
  /** Changes what is held for `scope` at once: the current scope when no scope is given, `undefined` being a scope
   * of its own when given. */
  hold: (change: (held: V) => V, ...scope: [] | [string | undefined]) => void;
  /** Gives the request out up and drops what is held. */
  reset: () => void;
  /** Whether any request has been sent. */
  asked: () => boolean;
  /** Whether the latest request is out, for a poll to wait for it. */
  busy: () => boolean;
  /** When the latest request was sent (`Date.now()`), for a poll to count from. */
  sentAt: () => number;
}

/** The mutable side of `useLatestRequest`, read by requests that outlive the render that sent them. */
interface Live<V> {
  alive: boolean;
  scope: string | undefined;
  sequence: number;
  out: boolean;
  sentAt: number;
  asked: boolean;
  lost: boolean;
  held: { scope: string | undefined; value: V };
}

interface LatestRequests<V> {
  live: { readonly current: Live<V> };
  hold: (change: (held: V) => V, ...scope: [] | [string | undefined]) => void;
  /** Sorts out a rejection, with the `t` of the latest render that worded it. */
  classify: (err: unknown) => { failure: BrowseFailure; t: Translate };
}

/** Sends `request` as the latest one, and again on `source_changed` until its retries are spent. At module level, as
 * it calls itself to retry, which the React Compiler would take for a call during render inside the hook. */
function sendLatest<V, R>(requests: LatestRequests<V>, request: LatestRequestSend<V, R>, attempt: number): Promise<void> {
  const { hold } = requests;
  const s = requests.live.current;
  const asked = "scope" in request ? request.scope : s.scope;
  const sequence = ++s.sequence;
  s.asked = true;
  s.lost = false;
  s.out = true;
  s.sentAt = Date.now();
  const current = () => s.alive && s.sequence === sequence && s.scope === asked && (request.wanted?.() ?? true);
  if (attempt === 0 && request.pending) hold(request.pending, asked);
  const sent = request.lane
    ? request.lane.send(() => (current() ? request.request() : undefined), { slot: request.slot })
    : request.request();
  return sent
    .then((reply) => {
      if (reply === undefined || !current()) return;
      const change = request.accept(reply);
      if (change) hold(change, asked);
    })
    .catch((err: unknown) => {
      if (!current() || request.handled?.(err)) return;
      const { failure, t } = requests.classify(err);
      if (failure.kind === "superseded") return;
      if (failure.kind === "changed" && attempt < CHANGED_RETRIES) return sendLatest(requests, request, attempt + 1);
      if (failure.kind === "disconnected") {
        s.lost = true;
        const disconnected = request.disconnected;
        if (disconnected) hold((value) => disconnected(value, t), asked);
        return;
      }
      hold((value) => request.failed(failure, value, t), asked);
    })
    .finally(() => {
      if (s.sequence === sequence) s.out = false;
    });
}

/**
 * Asks the daemon and keeps only the latest answer: the "latest request" pattern every view of daemon data that the
 * user moves through shares. The value held is scoped (a worktree, a pair of branches, a query, a file shown), and a
 * reply is taken only for the latest request, for the scope it was asked for, while the caller is mounted.
 *
 * A rejection is sorted out by `browseFailure` (`projectBrowseFailure` with `project`), worded in the language of the
 * latest render: one given up for a newer request in its slot is dropped; one whose source changed while it was read
 * is sent again up to `CHANGED_RETRIES` times, and then shown as `changed`; one lost with the connection keeps what
 * is held (`disconnected`) and is reported to `onReconnect`; any other is shown (`failed`). Each request says what is
 * held meanwhile (`pending`), so whether a refresh blanks the value, keeps it, or marks it as refreshing, and whether
 * a failure survives a background refresh, is the caller's rule.
 */
export function useLatestRequest<V>({ scope, initial, project, onReconnect }: LatestRequestOptions<V>): LatestRequest<V> {
  const t = useT();
  const language = useCurrentLanguage();
  // The wording a failure is given when it happens, not when the request was sent: the language can change between.
  const wording = useLatest({ t, language, project });
  const reconnected = useLatest(onReconnect);
  const { store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [held, setHeld] = useState<{ scope: string | undefined; value: V }>({ scope, value: initial });
  // `held` as last set, ahead of the render that commits it: an unchanged value is compared with it and never set,
  // since even an updater returning the state it was given can render the caller again.
  const live = useRef<Live<V>>({ alive: true, scope, sequence: 0, out: false, sentAt: 0, asked: false, lost: false, held });
  useLayoutEffect(() => {
    live.current.scope = scope;
  });

  const hold = (change: (held: V) => V, ...scope: [] | [string | undefined]) => {
    const s = live.current;
    const at = scope.length > 0 ? scope[0] : s.scope;
    const now = s.held;
    const next = change(now.scope === at ? now.value : initial);
    if (now.scope === at && Object.is(next, now.value)) return;
    s.held = { scope: at, value: next };
    setHeld(s.held);
  };

  const latestRequests: LatestRequests<V> = {
    live,
    hold,
    classify: (err) => {
      const { t, language, project } = wording.current;
      const state = store.getState();
      return { failure: project === undefined ? browseFailure(t, language, err, state) : projectBrowseFailure(t, language, err, state, project), t };
    },
  };
  const send = <R>(request: LatestRequestSend<V, R>) => sendLatest(latestRequests, request, 0);

  const reset = () => {
    const s = live.current;
    s.sequence += 1;
    s.out = false;
    s.lost = false;
    s.held = { scope: undefined, value: initial };
    setHeld(s.held);
  };

  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch === firstEpoch.current) return;
    const { asked, lost } = live.current;
    reconnected.current?.({ asked, lost });
  }, [snapshotEpoch, reconnected]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  return {
    value: held.scope === scope ? held.value : initial,
    send,
    hold,
    reset,
    asked: () => live.current.asked,
    busy: () => live.current.out,
    sentAt: () => live.current.sentAt,
  };
}
