import type { Translate } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import { joinPhrases } from "./i18n/joinPhrases";
import type { Language } from "./i18n/languages";
import type { GitStatus } from "./protocol";

/** Which phrase names the branch itself, chosen by `activity` and whether `branch` is known —
 * independent of `ahead`/`behind`, which `gitBadgeAriaLabel` appends as their own phrases.
 * `branch` is `null` for two different reasons (see `GitStatus.branch` in `protocol.ts`): a
 * freshly-`git init`'d repository with no commit yet, or a failed read. `error` tells the two
 * apart, since only a failed read sets it, and the badge's failure mark already says that the
 * check failed — so this returns `undefined` for a failed read rather than a second, emptier
 * phrase saying the same thing. */
function branchPhrase(t: Translate, status: GitStatus): string | undefined {
  const branch = status.branch == null ? undefined : isolate(status.branch);
  if (status.activity === "pushing") return branch ? t("sidebar.git.pushing", { branch }) : t("sidebar.git.pushingUnknown");
  if (status.activity === "checking") return branch ? t("sidebar.git.checking", { branch }) : t("sidebar.git.checkingUnknown");
  if (status.activity === "pulling") return branch ? t("sidebar.git.pulling", { branch }) : t("sidebar.git.pullingUnknown");
  if (branch) return status.detached ? t("sidebar.git.detachedBranch", { branch }) : t("sidebar.git.branch", { branch });
  return status.error ? undefined : t("sidebar.git.noCommitsYet");
}

/**
 * The git badge's accessible name, whose words are also the text of its one tooltip (which draws
 * the branch as an element rather than isolating it): the branch (or why there is none), `ahead` and
 * `behind` each only when they are not zero — independent facts, joined by `joinPhrases` rather than
 * assembled from translated word fragments, whose order a sentence built in code could not get right in
 * every language. A failed check is not part of it: the badge's failure mark is a control of its own,
 * named by `gitFailureLabel`, and Git's own text is only ever in that mark's popover. Empty for a failed
 * read with no branch and nothing in flight, where the mark is all the badge shows.
 */
export function gitBadgeAriaLabel(language: Language, t: Translate, status: GitStatus): string {
  const branch = branchPhrase(t, status);
  const phrases = branch ? [branch] : [];
  if (status.ahead > 0) phrases.push(t("sidebar.git.ahead", { count: status.ahead }));
  if (status.behind > 0) phrases.push(t("sidebar.git.behind", { count: status.behind }));
  return joinPhrases(language, phrases);
}

/** Whether the badge shows its failure mark: a failed check's error stays in `GitStatus` while the next check,
 * pull or push runs, but the mark gives way to the refresh glyph until that run ends; it is back afterwards unless a
 * check in between succeeded, which is what clears the error. */
export function gitFailureShown(status: GitStatus): status is GitStatus & { error: string } {
  return Boolean(status.error) && status.activity === "idle";
}

/** The name of the badge's failure mark, in the application's own words: `GitStatus.error` is Git's or
 * the system's raw text with no code to word it by, so the name is the same for every failure. */
export function gitFailureLabel(t: Translate): string {
  return t("sidebar.git.failed");
}

/**
 * A tree row's own accessible name with the git badge's facts folded in: a row announces itself
 * as a single `role="button"`, which keeps every descendant's own role and `aria-label` from ever
 * reaching assistive technology while the row is read, so the badge's facts have to be said here or
 * nowhere — the failure mark's name included, as its own phrase, although the mark is a control reached
 * on its own. Returns `label` unchanged when there is nothing to show (no status yet, or not a
 * repository).
 */
export function withGitBadge(language: Language, t: Translate, label: string, status: GitStatus | undefined): string {
  if (!status || !status.repository) return label;
  const badge = gitBadgeAriaLabel(language, t, status);
  return joinPhrases(language, [label, ...(badge ? [badge] : []), ...(gitFailureShown(status) ? [gitFailureLabel(t)] : [])]);
}
