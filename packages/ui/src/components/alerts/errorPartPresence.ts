import { leftBranchDetail } from "../../errors/branchCleanupItems";
import type { MessageParams } from "../../protocol";
import { daemonErrorParts } from "./daemonErrorParts";

// What `DaemonErrorMessage` has to show for each of its parts, answered without rendering from the same `daemonErrorParts`
// it renders from. Plain predicates, apart from the node builders in `errorParts.tsx` so that `DaemonErrorMessage` can ask
// them without importing the file that imports it.

type Failure = { code: string; params: MessageParams; message: string };

/** Whether `DaemonErrorMessage` has anything for `part="details"` of this failure, answered without rendering, so an
 * alert knows whether it has a Details button to offer. What counts is further explanation in the application's own
 * words (a reason for the cause, the lead of a group of names) and raw text: a diagnostic block, the entries of a
 * failure, the system's own message of a failure the application has no words for. */
export function hasErrorDetails(error: Failure): boolean {
  if (hasRawDetails(error)) return true;
  const parts = daemonErrorParts(error);
  if (parts.uncatalogued) return false;
  if (parts.shellTimeout) return true;
  if (!parts.worded) return false;
  if (parts.left || parts.further) return true;
  if (parts.sharing?.hasReasonLine) return true;
  return parts.nested !== undefined && hasErrorDetails(parts.nested);
}

/** Whether `DaemonErrorMessage` has anything for `part="raw"`: text not written for the reader (a diagnostic block,
 * the entries of a failure, the system's own message of a failure the application has no words for, an
 * `ActionableError`'s `detail`), answered without rendering, so a surface knows whether it has a disclosure to offer. */
export function hasRawDetails(error: Failure): boolean {
  const parts = daemonErrorParts(error);
  if (parts.uncatalogued) return error.message !== "";
  if (parts.actionableDetail) return true;
  if (!parts.worded) return false;
  if (parts.left) return parts.left.some((item) => !!leftBranchDetail(item));
  if (parts.sharing?.items || parts.sharing?.raw || parts.diagnostic || parts.unworded) return true;
  return parts.nested !== undefined && hasRawDetails(parts.nested);
}

/** Whether `DaemonErrorMessage` has anything for `part="remedy"`: the failure's recovery sentence, the one
 * main thing to do about it, answered without rendering. */
export function hasErrorRemedy(error: Failure): boolean {
  const parts = daemonErrorParts(error);
  if (parts.uncatalogued) return false;
  if (parts.shellTimeout) return true;
  if (!parts.worded) return false;
  if (parts.recovery) return true;
  return parts.nestedRemedy && parts.nested !== undefined && hasErrorRemedy(parts.nested);
}

/** Whether an alert's default body already carries the failure's remedy sentence, so the alert adds no second one:
 * the body is the default one (the caller gave no `content` of its own) and is a structured failure with a remedy. */
export function remedyInBody(content: unknown, shown: string | { code: string; params: MessageParams; message: string }): boolean {
  return content === undefined && typeof shown !== "string" && hasErrorRemedy(shown);
}
