import { useEffect, useRef } from "react";

import { listLane } from "../../browse/budget";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import type { ComparisonEndpoint } from "../../protocol";
import { useDaemon } from "../../store";
import { changeItems, type ChangeItem } from "./changes";
import { gitMoment } from "./gitRequests";

/**
 * A comparison of two branches as the client holds it. Once loaded, `left` and `right` are the
 * branches with the commits they were resolved to, `base` is the commit they forked at, and every
 * change in `items` is between exactly `base` and `right`'s commit; `receivedAt` is when that
 * comparison was accepted (`gitMoment`). A comparison stays on screen while it is asked for
 * again, and so does a failure while it is asked for in the background.
 */
export type Comparison =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "loaded"; left: ComparisonEndpoint; right: ComparisonEndpoint; base: string; items: ChangeItem[]; complete: boolean; receivedAt: number }
  | { state: "error"; message: ErrorPresentation };

/**
 * The comparison of branch `right` with branch `left`, its base, of a project: what `right` has
 * changed since it forked from `left`, from their merge base to `right`'s tip. `left` is a
 * remote-tracking branch when `leftKind` says `remote`, and nothing, the empty tree, when `empty`. It is asked of the
 * daemon once both are known and the comparison is `active`, and again on `refresh` and `update`.
 * The branches are resolved to their commits, and those to their merge base, when it is asked for,
 * and the commits it answers with stay until it is asked for again: a branch that moves meanwhile
 * never repoints it, and the caller asks for it again (`update`) once it knows one has. Other
 * branches are another comparison, whose state starts afresh; a reply is taken only for the latest
 * request and the branches it was asked for. A request lost with the connection is sent again once
 * the connection is back; a comparison already held is kept as it is. `refresh` and `update` both
 * keep a comparison on screen until the new one is in, and `update`, being no request of the
 * user's, keeps a failure on screen too; either settles once the comparison has been answered or
 * has failed, or when a newer request has taken its place.
 */
export function useBranchComparison(
  project: string,
  left: string | undefined,
  leftKind: "local" | "remote" | "empty",
  right: string | undefined,
  active: boolean,
): { comparison: Comparison; refresh: () => Promise<void>; update: () => Promise<void> } {
  const { request } = useDaemon();
  // What is held is scoped to the branches it is of: a comparison of other branches is not this one's.
  const pair = left === undefined || right === undefined ? undefined : `${left}\0${leftKind}\0${right}`;
  const latest = useLatestRequest<Comparison>({
    scope: pair,
    initial: { state: "loading" },
    project,
    onReconnect: ({ lost }) => {
      if (lost && pair !== undefined) void send(true);
    },
  });
  const sent = useRef<string | undefined>(undefined);

  const send = (background: boolean): Promise<void> => {
    if (left === undefined || right === undefined) return Promise.resolve();
    sent.current = pair;
    const slot = `comparison:${project}`;
    return latest.send({
      lane: listLane,
      slot,
      request: () =>
        request({ type: "compare_project_branches", project, left, left_remote: leftKind === "remote" || undefined, left_empty: leftKind === "empty" || undefined, right, slot }),
      pending: (comparison) => (comparison.state === "loaded" || (comparison.state === "error" && background) ? comparison : { state: "loading" }),
      accept: (reply) => {
        if (reply.project !== project || reply.left.branch !== left || reply.right.branch !== right) return undefined;
        const accepted = { left: reply.left, right: reply.right, base: reply.base, complete: reply.complete, receivedAt: gitMoment() };
        return () => ({ state: "loaded", ...accepted, items: changeItems(reply.changes) });
      },
      failed: (failure, _held, t) => ({ state: "error", message: failure.kind === "failed" ? failure.message : t("git.error.changing") }),
    });
  };

  // Other branches are another comparison: nothing of the previous one stands.
  useEffect(() => {
    if (active && pair !== undefined && sent.current !== pair) void send(false);
  }, [active, pair]);

  const comparison: Comparison = pair === undefined ? { state: "idle" } : latest.value;
  return {
    comparison,
    refresh: () => (pair !== undefined ? send(false) : Promise.resolve()),
    update: () => (pair !== undefined ? send(true) : Promise.resolve()),
  };
}
