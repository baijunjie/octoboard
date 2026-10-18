import { useEffect, useLayoutEffect, useRef, useState } from "react";

import type { ErrorPresentation } from "../errorPresentation";
import { useCurrentLanguage, useT } from "../i18n/react";
import { abbreviateHome } from "../pathDisplay";
import { useDaemon, useDaemonStore } from "../store";
import { browseFailure } from "./browseError";
import { sendBrowseListRequest } from "./browseListRequests";
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
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  // What is held, with the branch and the base branch it is of: a list of other commits of the
  // same branch against the same base branch may stand in for the new one's, one of another line
  // may not.
  const line = range === undefined ? undefined : `${range.baseBranch}\0${range.branch}`;
  const key = range === undefined ? undefined : `${line}\0${range.base}..${range.tip}`;
  const [held, setHeld] = useState<{ line: string | undefined; list: CommitList }>({ line, list: { state: "loading" } });
  const live = useRef({ alive: true, sequence: 0, key, lost: false, sent: undefined as string | undefined });
  useLayoutEffect(() => {
    live.current.key = key;
  });

  const send = (): Promise<void> => {
    const s = live.current;
    if (range === undefined || key === undefined) return Promise.resolve();
    const { base, tip } = range;
    const setList = (change: (list: CommitList) => CommitList) =>
      setHeld((now) => ({ line, list: change(now.line === line ? now.list : { state: "loading" }) }));
    const sequence = ++s.sequence;
    s.sent = key;
    s.lost = false;
    const current = () => s.alive && s.sequence === sequence && s.key === key;
    setList((list) => (list.state === "loaded" ? list : { state: "loading" }));
    const slot = `commits:${project}`;
    return sendBrowseListRequest(slot, () => (current() ? request({ type: "list_project_commits", project, base, tip, slot }) : undefined))
      .then((reply) => {
        if (!reply || !current() || reply.type !== "project_commits" || reply.project !== project) return;
        if (reply.base !== base || reply.tip !== tip) return;
        setList(() => ({
          state: "loaded",
          base,
          tip,
          items: commitItems(reply.commits),
          count: reply.commits.length,
          complete: reply.complete,
          commitsComplete: reply.commits_complete,
        }));
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
        setList(() => ({ state: "error", message: failure.kind === "failed" ? failure.message : t("git.error.changing") }));
      });
  };

  useEffect(() => {
    if (active && key !== undefined && live.current.sent !== key) send();
  }, [active, key]);

  const firstEpoch = useRef(snapshotEpoch);
  useEffect(() => {
    if (snapshotEpoch !== firstEpoch.current && live.current.lost && active && key !== undefined) send();
  }, [snapshotEpoch]);

  useEffect(() => {
    const s = live.current;
    s.alive = true;
    return () => {
      s.alive = false;
    };
  }, []);

  return {
    commits: range === undefined ? { state: "idle" } : held.line === line ? held.list : { state: "loading" },
    refresh: () => (active && key !== undefined ? send() : Promise.resolve()),
  };
}
