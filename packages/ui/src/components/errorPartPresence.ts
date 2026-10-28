import { ActionableError } from "../actionableError";
import { DETAIL_BLOCK_CODES, RECOVERY_CODES, hasRichMessage, hasTrustDiagnostic, leftBranchDetail, leftBranchItems, messageCode, sharingFailureItems, sharingRawReason, sharingReasonKey } from "../daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, unwordedReason } from "../errorPresentation";
import { isUncataloguedFailure } from "../uncataloguedFailure";
import { isMessageKey } from "../i18n/catalog";
import { NESTED_FAILURE_CODES, NESTED_REMEDY_CODES, type MessageParams } from "../protocol";

// What `DaemonErrorMessage` has to show for each of its parts, answered without rendering. Plain predicates, apart from
// the node builders in `errorParts.tsx` so that `DaemonErrorMessage` can ask them without importing the file that
// imports it.

/** Whether `DaemonErrorMessage` has anything for `part="details"` of this failure: the same conditions
 * `CodedDaemonMessage` builds those blocks under, answered without rendering, so an alert knows whether
 * it has a Details button to offer. What counts is further explanation in the application's own words
 * (a reason for the cause, the lead of a group of names) and raw text: a diagnostic block, the entries
 * of a failure, the system's own message of a failure the application has no words for. */
export function hasErrorDetails(error: { code: string; params: MessageParams; message: string }): boolean {
  if (hasRawDetails(error)) return true;
  if (isUncataloguedFailure(error)) return false;
  if (hasShellTimeoutDetails(error)) return true;
  if (!hasCodeParams(error) || !hasRichMessage(error.code, error.params)) return false;
  if (leftBranchItems(error.code, error.params)) return true;
  if (isMessageKey(`daemon.${error.code}.details`)) return true;
  if (sharingRawReason(error) && sharingReasonKey(error.params.cause)) return true;
  const nested = nestedFailure(error);
  return nested !== undefined && hasErrorDetails(nested);
}

/** Whether `DaemonErrorMessage` has anything for `part="raw"`: text not written for the reader (a diagnostic block,
 * the entries of a failure, the system's own message of a failure the application has no words for, an
 * `ActionableError`'s `detail`), answered without rendering, so a surface knows whether it has a disclosure to offer. */
export function hasRawDetails(error: { code: string; params: MessageParams; message: string }): boolean {
  if (isUncataloguedFailure(error)) return error.message !== "";
  if (error instanceof ActionableError && error.detail) return true;
  if (hasShellTimeoutDetails(error)) return false;
  if (!hasCodeParams(error) || !hasRichMessage(error.code, error.params)) return false;
  const left = leftBranchItems(error.code, error.params);
  if (left) return left.some((item) => !!leftBranchDetail(item));
  const sharing = sharingRawReason(error);
  if (sharingFailureItems(error.code, error.params) || sharing?.raw) return true;
  if (!sharing && (DETAIL_BLOCK_CODES.has(error.code) || hasTrustDiagnostic(error.code, error.params)) && error.params.detail) return true;
  if (unwordedReason(error.code, error.params)) return true;
  const nested = nestedFailure(error);
  return nested !== undefined && hasRawDetails(nested);
}

/** Whether `DaemonErrorMessage` has anything for `part="remedy"`: the failure's recovery sentence, the one
 * main thing to do about it, answered without rendering. */
export function hasErrorRemedy(error: { code: string; params: MessageParams; message: string }): boolean {
  if (isUncataloguedFailure(error)) return false;
  if (hasShellTimeoutDetails(error)) return true;
  if (!hasCodeParams(error) || !hasRichMessage(error.code, error.params)) return false;
  if (RECOVERY_CODES.has(error.code) && isMessageKey(`daemon.${messageCode(error.code, error.params)}.recovery`)) return true;
  const nested = nestedFailure(error);
  return nested !== undefined && NESTED_REMEDY_CODES.has(error.code) && hasErrorRemedy(nested);
}

/** Whether an alert's default body already carries the failure's remedy sentence, so the alert adds no second one:
 * the body is the default one (the caller gave no `content` of its own) and is a structured failure with a remedy. */
export function remedyInBody(content: unknown, shown: string | { code: string; params: MessageParams; message: string }): boolean {
  return content === undefined && typeof shown !== "string" && hasErrorRemedy(shown);
}

/** The structured cause a `launch_failed` or a failed automatic switch carries under its own code, as a failure of its own. */
export function nestedFailure(error: { code: string; params: MessageParams }): { code: string; params: MessageParams; message: string } | undefined {
  const { reason_code: reason } = error.params;
  if (!NESTED_FAILURE_CODES.has(error.code) || !reason || reason === error.code) return undefined;
  const nested = { code: reason, params: error.params, message: error.params.reason ?? "" };
  return hasCodeParams(nested) || hasShellTimeoutDetails(nested) ? nested : undefined;
}
