import { ActionableError } from "../../errors/actionableError";
import { pathLinkRefusedCause } from "../../errors/causes";
import { CODE } from "../../errors/codes";
import { CodedError } from "../../errors/codedError";
import type { Translate } from "../../i18n/catalog";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { Message } from "../../i18n/react";
import type { AlertOptions } from "../../store";
import { raiseError } from "./raiseError";

/** Whether `error` is the daemon's refusal of a file the system could launch, as a path link's. */
function isLaunchableRefusal(error: unknown): error is CodedError {
  return error instanceof CodedError && error.code === CODE.path_link_refused && pathLinkRefusedCause(error.params.cause) === "launchable";
}

/** Raises the error alert for a press on Default application that failed (the viewer header's, the project pane row's, a
 * path link's Open in), with `recovery` for the failure. The daemon refuses a file the system could launch, and its
 * sentence for that refusal says the path is not opened from a link, which a button is not: that refusal is worded for a
 * button, with the path as a chip in the alert's body (the message keeps it as text for the report). Any other failure
 * is raised as it is. */
export function raiseDefaultApplicationFailure(t: Translate, error: unknown, recovery: AlertOptions["recovery"]): void {
  const title = t("common.openDefaultApplicationFailedTitle");
  if (!isLaunchableRefusal(error)) return raiseError(error, { title, recovery });
  const message = new ActionableError(t("common.defaultApplicationLaunchable", { path: isolate(error.params.path ?? "") }), "pathLinkLaunchable", { cause: error });
  raiseError(message, {
    title,
    recovery,
    content: <Message id="common.defaultApplicationLaunchable" params={{ path: <InlineReference monospace dir="ltr" wrap>{error.params.path ?? ""}</InlineReference> }} />,
  });
}
