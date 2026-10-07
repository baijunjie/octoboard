import type { Translate } from "./i18n/catalog";
import { joinPhrases } from "./i18n/joinPhrases";
import type { Language } from "./i18n/languages";
import type { GitStatus } from "./protocol";

/** Which phrase names the branch itself, chosen by `activity` and whether `branch` is known —
 * independent of `ahead`/`behind`, which `gitBadgeAriaLabel` appends as their own phrases.
 * `branch` is `null` for two different reasons (see `GitStatus.branch` in `protocol.ts`): a
 * freshly-`git init`'d repository with no commit yet, or a failed read. `error` tells the two
 * apart, since only a failed read sets it, and `gitBadgeAriaLabel` already appends the error
 * phrase from it — so this returns `undefined` for a failed read rather than a second, emptier
 * phrase saying the same thing. */
function branchPhrase(t: Translate, status: GitStatus): string | undefined {
  const branch = status.branch ?? undefined;
  if (status.activity === "checking") return branch ? t("sidebar.git.checking", { branch }) : t("sidebar.git.checkingUnknown");
  if (status.activity === "syncing") return branch ? t("sidebar.git.syncing", { branch }) : t("sidebar.git.syncingUnknown");
  if (branch) return status.detached ? t("sidebar.git.detachedBranch", { branch }) : t("sidebar.git.branch", { branch });
  return status.error ? undefined : t("sidebar.git.noCommitsYet");
}

/**
 * The git badge's accessible name: the branch (or why there is none), `ahead` and `behind` each
 * only when they are not zero, and the last error, if any — independent facts, joined by
 * `joinPhrases` rather than assembled from translated word fragments, whose order a
 * sentence built in code could not get right in every language.
 */
export function gitBadgeAriaLabel(language: Language, t: Translate, status: GitStatus): string {
  const branch = branchPhrase(t, status);
  const phrases = branch ? [branch] : [];
  if (status.ahead > 0) phrases.push(t("sidebar.git.ahead", { count: status.ahead }));
  if (status.behind > 0) phrases.push(t("sidebar.git.behind", { count: status.behind }));
  if (status.error) phrases.push(t("sidebar.git.error", { error: status.error }));
  return joinPhrases(language, phrases);
}

/**
 * A tree row's own accessible name with the git badge's facts folded in: a row announces itself
 * as a single `role="button"`, which keeps every descendant's own role and `aria-label` from ever
 * reaching assistive technology, so the badge's facts have to be said here or nowhere. Returns
 * `label` unchanged when there is nothing to show (no status yet, or not a repository).
 */
export function withGitBadge(language: Language, t: Translate, label: string, status: GitStatus | undefined): string {
  if (!status || !status.repository) return label;
  return joinPhrases(language, [label, gitBadgeAriaLabel(language, t, status)]);
}
