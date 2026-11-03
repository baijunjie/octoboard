import { gitMoment } from "./gitRequests";

/** What a session set in a project's Git mode branch selector, kept for the window's lifetime so a
 * pane mounted again under the same selection does not set it again over the user's choice. Read
 * and written from effects and handlers only, never during render. */
export interface SessionFollow {
  session: string;
  /** The selection (`noteSessionSwitch`) this record is of: a later one makes it a new switch. */
  switchId: number;
  /** When that switch was made (`gitMoment`): only what was read from the daemon after it is
   * weighed. */
  since: number;
  /** The same for the worktree list, moved forward when it is asked again for the worktree the
   * session's branch should be checked out in. */
  sourceSince: number;
  /** Whether the pane has asked again for what was read before the switch. */
  asked: boolean;
  /** Whether the worktree list has been asked again for a worktree no listed one is. */
  retried: boolean;
  /** Set once nothing is left to set, or the user has chosen a branch. */
  settled: boolean;
}

const follows = new Map<string, SessionFollow>();
let switchCount = 0;
let lastSwitchAt = 0;

/** Records that the selected session changed, so each project's selector follows it afresh. */
export function noteSessionSwitch(): void {
  switchCount += 1;
  lastSwitchAt = gitMoment();
}

/** The record of `session` being followed in `project`, new for each selection of it. */
export function sessionFollow(project: string, session: string): SessionFollow {
  const held = follows.get(project);
  if (held && held.session === session && held.switchId === switchCount) return held;
  const made: SessionFollow = { session, switchId: switchCount, since: lastSwitchAt, sourceSince: lastSwitchAt, asked: false, retried: false, settled: false };
  follows.set(project, made);
  return made;
}

/** Drops the records of every project not among `projects`. */
export function forgetSessionFollowsOtherThan(projects: ReadonlySet<string>): void {
  for (const project of [...follows.keys()]) if (!projects.has(project)) follows.delete(project);
}
