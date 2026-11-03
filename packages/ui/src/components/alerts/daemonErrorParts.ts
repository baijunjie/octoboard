import { ActionableError } from "../../errors/actionableError";
import { leftBranchItems, type LeftBranchItem } from "../../errors/branchCleanupItems";
import { CodedError } from "../../errors/codedError";
import { codeRecord } from "../../errors/codes";
import { furtherKey, hasRichMessage, hasTrustDiagnostic, messageCode } from "../../errors/daemonMessage";
import { hasCodeParams, hasShellTimeoutDetails, uncataloguedSummaryKey, unwordedReason } from "../../errors/errorPresentation";
import { sharingFailureItems, sharingRawReason, sharingReasonKey, type SharingFailureItem } from "../../errors/sharingFailure";
import { isUncataloguedFailure } from "../../errors/uncataloguedFailure";
import { isMessageKey, type MessageKey, type PlainMessageKey } from "../../i18n/catalog";
import type { MessageParams } from "../../protocol";

type Failure = { code: string; params: MessageParams; message: string };

/** What a structured daemon failure has to show, part by part, worked out without rendering. `DaemonErrorMessage`
 * builds its parts from these descriptors and `errorPartPresence` answers whether a part is there from the same ones, so
 * the two cannot disagree. Only a `worded` failure (the catalog words it, with every chip it needs) has the descriptors
 * below `worded`. */
export type DaemonErrorParts = {
  /** The application has no message of its own for the failure (see `isUncataloguedFailure`). */
  uncatalogued: boolean;
  /** The failure is the shell-environment timeout with its command, which has a layout of its own. */
  shellTimeout: boolean;
  worded: boolean;
  /** An `ActionableError`'s `detail`, raw text of its own whatever the catalog says of its code. */
  actionableDetail?: string;
  /** The entries of a failure to remove branches or worktrees. */
  left?: LeftBranchItem[];
  /** The failure's further explanation in the application's own words. */
  further?: MessageKey;
  /** For a sharing failure, its entries, the raw text that says why the change failed as a whole, and whether its
   * cause has a line of its own to show. */
  sharing?: { items?: SharingFailureItem[]; raw?: string; hasReasonLine: boolean };
  /** Verbatim diagnostic output the failure carries. */
  diagnostic?: string;
  /** A cause the application has no words for: its raw text, and the generic sentence that stands for it. */
  unworded?: { raw: string; summary: PlainMessageKey };
  /** The catalog key of the failure's recovery sentence. */
  recovery?: MessageKey;
  /** The failure that a `launch_failed` or a failed automatic switch carries under its own code, as a failure of its
   * own, when the application words it. */
  nested?: Failure;
  /** The remedy of the nested failure is the failure's own, a launch failure having none of its own. */
  nestedRemedy: boolean;
};

/** The structured cause a `launch_failed` or a failed automatic switch carries under its own code, as a failure of its own. */
function nestedFailure(error: { code: string; params: MessageParams }): Failure | undefined {
  const { reason_code: reason } = error.params;
  if (!codeRecord(error.code)?.nested || !reason || reason === error.code) return undefined;
  const nested = { code: reason, params: error.params, message: error.params.reason ?? "" };
  return hasCodeParams(nested) || hasShellTimeoutDetails(nested) ? nested : undefined;
}

/** Works out the descriptors of `error` (see `DaemonErrorParts`) from the catalog and the failure's params alone. */
export function daemonErrorParts(error: Failure): DaemonErrorParts {
  const uncatalogued = isUncataloguedFailure(error);
  const shellTimeout = hasShellTimeoutDetails(error);
  const worded = !uncatalogued && !shellTimeout && hasCodeParams(error) && hasRichMessage(error.code, error.params);
  const actionableDetail = error instanceof ActionableError && error.detail ? error.detail : undefined;
  if (!worded) return { uncatalogued, shellTimeout, worded, actionableDetail, nestedRemedy: false };
  const record = codeRecord(error.code);
  const reason = sharingRawReason(error);
  const unwordedRaw = unwordedReason(error.code, error.params);
  const recovery = `daemon.${messageCode(error.code, error.params)}.recovery`;
  const diagnostic = !reason && (record?.detailBlock || hasTrustDiagnostic(error.code, error.params)) && error.params.detail ? error.params.detail : undefined;
  return {
    uncatalogued,
    shellTimeout,
    worded,
    actionableDetail,
    left: leftBranchItems(error.code, error.params),
    further: furtherKey(error.code, error.params),
    sharing: reason && { items: sharingFailureItems(error.code, error.params), raw: reason.raw, hasReasonLine: sharingReasonKey(error.params.cause) !== undefined },
    diagnostic,
    unworded: unwordedRaw === undefined ? undefined : { raw: unwordedRaw, summary: record?.nested ? uncataloguedSummaryKey(new CodedError(unwordedRaw, error.params.reason_code ?? error.code, error.params)) : "daemon.trust_reason.unexpected" },
    recovery: record?.recoveryLine && isMessageKey(recovery) ? recovery : undefined,
    nested: nestedFailure(error),
    nestedRemedy: record?.nested === "failureAndRemedy",
  };
}
