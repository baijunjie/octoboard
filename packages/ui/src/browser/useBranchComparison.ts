import { useEffect, useRef, useState } from "react";

import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import type { ComparisonEndpoint } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { changeItems, type ChangeItem } from "./changes";
import { sendGitRequest } from "./gitRequests";

/**
 * A comparison of two branches as the client holds it. Once loaded, `left` and `right` are the
 * branches with the commits they were resolved to, and every change in `items` is between exactly
 * those; `receivedAt` is when that pair was accepted. A comparison stays on screen while it is
 * asked for again, and so does a failure while it is asked for in the background.
 */
export type Comparison =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "loaded"; left: ComparisonEndpoint; right: ComparisonEndpoint; items: ChangeItem[]; complete: boolean; receivedAt: number }
  | { state: "error"; message: string };

/**
 * The comparison of branch `left` (the old side) with branch `right` (the new side) of a project,
 * asked of the daemon once both are chosen and the comparison is `active`, and again only on
 * `refresh`. The branches are resolved to their commits when it is asked for, and the pair it
 * answers with stays until it is asked for again: a branch that moves meanwhile never repoints it.
 * Choosing other branches is another comparison, whose state starts afresh; a reply is taken only
 * for the latest request and the branches it was asked for. A request lost with the connection is
 * sent again once the connection is back; a comparison already held is kept as it is.
 */
export function useBranchComparison(
  project: string,
  left: string | undefined,
  right: string | undefined,
  active: boolean,
): { comparison: Comparison; refresh: () => void } {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const pair = left === undefined || right === undefined ? undefined : `${left}\0${right}`;
  // What is held, with the branches it is of: a comparison of other branches is not this one's.
  const [held, setHeld] = useState<{ pair: string | undefined; comparison: Comparison }>({ pair, comparison: { state: "loading" } });
  const live = useRef({ alive: true, sequence: 0, pair, lost: false, sent: undefined as string | undefined });
  live.current.pair = pair;

  const send = (background: boolean) => {
    const s = live.current;
    if (left === undefined || right === undefined) return;
    const sequence = ++s.sequence;
    const asked = s.pair;
    s.sent = asked;
    s.lost = false;
    const current = () => s.alive && s.sequence === sequence && s.pair === asked;
    const setComparison = (change: (comparison: Comparison) => Comparison) =>
      setHeld((now) => ({ pair: asked, comparison: change(now.pair === asked ? now.comparison : { state: "loading" }) }));
    setComparison((comparison) => (comparison.state === "loaded" || (comparison.state === "error" && background) ? comparison : { state: "loading" }));
    const slot = `comparison:${project}`;
    sendGitRequest(slot, () => (current() ? request({ type: "compare_project_branches", project, left, right, slot }) : undefined))
      .then((reply) => {
        if (!reply || !current() || reply.type !== "project_comparison" || reply.project !== project) return;
        if (reply.left.branch !== left || reply.right.branch !== right) return;
        const accepted = { left: reply.left, right: reply.right, complete: reply.complete, receivedAt: performance.now() };
        setComparison(() => ({ state: "loaded", ...accepted, items: changeItems(reply.changes) }));
      })
      .catch((err: unknown) => {
        if (!current()) return;
        const state = store.getState();
        const root = state.projects.get(project)?.path;
        const failure = browseFailure(t, language, err, state, root && abbreviateHome(root, state.homeDir));
        if (failure.kind === "superseded") return;
        if (failure.kind === "disconnected") {
          // Left as it is; the reconnect asks again.
          s.lost = true;
          return;
        }
        const message = failure.kind === "failed" ? failure.message : t("git.error.changing");
        setComparison(() => ({ state: "error", message }));
      });
  };

  // Other branches are another comparison: nothing of the previous one stands.
  useEffect(() => {
    if (active && pair !== undefined && live.current.sent !== pair) send(false);
  }, [active, pair]);

  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch !== firstEpoch.current && live.current.lost && pair !== undefined) send(true);
  }, [snapshotEpoch]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  const comparison: Comparison = pair === undefined ? { state: "idle" } : held.pair === pair ? held.comparison : { state: "loading" };
  return {
    comparison,
    refresh: () => {
      if (pair !== undefined) send(false);
    },
  };
}
