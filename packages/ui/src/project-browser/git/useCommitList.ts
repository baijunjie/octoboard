import { useEffect, useLayoutEffect, useRef } from "react";

import { listLane } from "../../browse/budget";
import { useLatestRequest } from "../../components/hooks/useLatestRequest";
import type { ErrorPresentation } from "../../errors/errorPresentation";
import { useDaemon } from "../../store";
import { commitItems, type ChangeItem } from "./changes";

/**
 * The commits a branch made since it forked, as the client holds them. Once loaded, `base` and
 * `tip` are the commits the list runs between, and `items` are the commits' changes, by commit
 * (`commitItems`); `complete` is false when the changes were cut, `commitsComplete` when
 * only the newest commits were listed.
 */
export type CommitList =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "loaded"; base: string; tip: string; items: ChangeItem[]; count: number; complete: boolean; commitsComplete: boolean }
  | { state: "error"; message: ErrorPresentation };

/**
 * The commits from `range.base`, a comparison's merge base, to `range.tip`, its right commit, each
 * with its own changes, asked of the daemon while `active` and whenever the range changes, and on
 * `refresh`. A range is two commits, which never change, so nothing is asked again on a timer: a
 * branch that moves is another range, which the comparison made again names. The list held stays
 * on screen while another range of the same `range.branch` against the same `range.baseBranch` is
 * asked for, until its reply is in, so making the comparison again does not blank the sections;
 * the list of another branch, or of the same branch against another base branch, never stands for
 * this one's, and a failure gives way to the new range's reply. A reply is taken only for the
 * latest request and the range it was asked for. A request lost with the connection is sent
 * again once the connection is back. `refresh` settles once the list has been answered or has
 * failed, or when a newer request has taken its place.
 */
export function useCommitList(
  project: string,
  range: { branch: string; baseBranch: string; base: string; tip: string } | undefined,
  active: boolean,
): { commits: CommitList; refresh: () => Promise<void> } {
  const { request } = useDaemon();
  // What is held is scoped to the branch and the base branch it is of: a list of other commits of
  // the same branch against the same base branch may stand in for the new one's, one of another
  // line may not. The request is for the range, of which there is one per line at a time.
  const line = range === undefined ? undefined : `${range.baseBranch}\0${range.branch}`;
  const key = range === undefined ? undefined : `${line}\0${range.base}..${range.tip}`;
  const latest = useLatestRequest<CommitList>({
    scope: line,
    initial: { state: "loading" },
    project,
    onReconnect: ({ lost }) => {
      if (lost && active && key !== undefined) void send();
    },
  });
  const sent = useRef<string | undefined>(undefined);
  const liveKey = useRef(key);
  useLayoutEffect(() => {
    liveKey.current = key;
  });

  const send = (): Promise<void> => {
    if (range === undefined || key === undefined) return Promise.resolve();
    const { base, tip } = range;
    sent.current = key;
    const slot = `commits:${project}`;
    return latest.send({
      lane: listLane,
      slot,
      wanted: () => liveKey.current === key,
      request: () => request({ type: "list_project_commits", project, base, tip, slot }),
      pending: (list) => (list.state === "loaded" ? list : { state: "loading" }),
      accept: (reply) => {
        if (reply.type !== "project_commits" || reply.project !== project || reply.base !== base || reply.tip !== tip) return undefined;
        return () => ({
          state: "loaded",
          base,
          tip,
          items: commitItems(reply.commits),
          count: reply.commits.length,
          complete: reply.complete,
          commitsComplete: reply.commits_complete,
        });
      },
      failed: (failure, _held, t) => ({ state: "error", message: failure.kind === "failed" ? failure.message : t("git.error.changing") }),
    });
  };

  useEffect(() => {
    if (active && key !== undefined && sent.current !== key) void send();
  }, [active, key]);

  return {
    commits: range === undefined ? { state: "idle" } : latest.value,
    refresh: () => (active && key !== undefined ? send() : Promise.resolve()),
  };
}
