import React, { useState } from "react";

import { AttendedStatusAnnouncer } from "../components/announce/AttendedStatusAnnouncer";
import { useStableHandler } from "../components/hooks/useLatest";
import { CODE } from "../errors/codes";
import { gitFailureLabel } from "../git/gitStatusLabel";
import { useT } from "../i18n/react";
import type { GitStatus } from "../protocol";

/** Told what a Try again on a project's Git-missing mark was answered with: the project it asked about, and the
 * status the check that answered ended with (`project_git_rechecked`), `null` when that check did not end in time. */
export type GitRetried = (project: string, status: GitStatus | null) => void;

/** Whether `status`, a re-check's answer, is a check that ended without finding `git`. A missing status (a stand-in
 * daemon answering with a bare `ack`) says nothing either way. */
function stillMissing(status: GitStatus | null | undefined): boolean {
  return Boolean(status?.repository && status.activity === "idle" && status.error_code === CODE.binary_not_found);
}

/**
 * The status region a placement of `GitBadge` mounts for `project`, and the `onRetried` to hand the badge: a Try
 * again on the Git-missing mark whose check ends without finding `git` again is spoken once, as the mark's name
 * ("Git check failed"), which is the mark that comes back; a check that finds `git` says nothing, the badge showing
 * the branch, and a sweep's check, which no request answers, is never spoken. It goes by the request's answer, not by
 * the statuses broadcast meanwhile, which a check in flight may never show.
 *
 * The placement mounts `announcer` outside a row's `role="button"`, whose children WebKit takes as presentational and
 * never reads, for as long as the placement lives, so the region is there, empty, before anything is written to it.
 * The answer can take a while (a fresh shell capture, a wait behind a running check), so the user may have left the
 * window: `AttendedStatusAnnouncer` writes the text `AFTER_FOCUS_MOVE_DELAY_MS` of attended time after the answer, on
 * every retry. The press closes the popover, and the check starting takes the mark away, handing focus to the row or
 * the header; VoiceOver drops a status written within about 300 ms of a focus move, and the answer comes once the
 * check has ended, after both. It is emptied again after it has been read (`clearAfterRead`), since the mark it
 * names is on the screen. A region set to the words it already holds does not change and says nothing, so every
 * other failing answer ends in a no-break space, which makes the text differ without changing what is spoken, and a
 * retry that fails again is heard again — once the one before it was written: two failing answers within one wait can
 * leave the second in the same words as the last written text, and then it is not heard.
 *
 * The announcement belongs to the project it was asked about: an answer for another project than the placement now
 * shows (a placement reused for the next focused project) is dropped, as is a wait or a text left from before the
 * switch.
 */
export function useGitRetryAnnouncement(project: string): { announcer: React.ReactElement; onRetried: GitRetried } {
  const t = useT();
  // `count` goes up with every failing answer, so the next text differs from the one before it; `speaks` is whether
  // the latest answer is to be spoken.
  const [said, setSaid] = useState<{ project: string; count: number; speaks: boolean }>();
  if (said && said.project !== project) setSaid(undefined);
  const onRetried = useStableHandler((asked: string, status: GitStatus | null) => {
    if (asked !== project) return;
    const speaks = stillMissing(status);
    setSaid((previous) => {
      const count = previous?.project === asked ? previous.count : 0;
      return { project: asked, count: speaks ? count + 1 : count, speaks };
    });
  });
  const label = gitFailureLabel(t);
  const text = said?.project === project && said.speaks ? (said.count % 2 === 0 ? `${label}\u00A0` : label) : undefined;
  return { announcer: <AttendedStatusAnnouncer text={text} clearAfterRead />, onRetried };
}
