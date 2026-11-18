import type { Translate } from "../i18n/catalog";
import type { PullOutcome } from "../protocol";

/** What a manual pull that changed nothing or pulled the branch says, as the notice that follows it. A pull that needs
 * a merge asks the person first and has no notice of its own. */
export function pullNotice(t: Translate, outcome: Exclude<PullOutcome, { kind: "needs_merge" }>): string {
  return t(outcome.kind === "up_to_date" ? "sidebar.project.pullUpToDate" : "sidebar.project.pulled");
}
