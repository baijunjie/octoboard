import { pathLinkRecovery } from "../../errors/errorRecovery";
import { raiseError } from "../alerts/raiseError";
import { t } from "../../i18n/language";
import type { RequestSender } from "../../requests";

/**
 * Has the daemon show `path`, given by a path link wherever the user pressed it (a terminal's link, the header of the
 * viewer the link opened), in the host's file manager: a file selected in its folder and a folder opened, whatever the
 * link's click would do with it. A failure is reported under the file manager's failure title, with the remedy of a
 * failed link open.
 */
export function revealPathTarget({ path, project, request }: {
  /** The link's path, as the link gives it. */
  path: string;
  /** The session's project, if it has one. */
  project: string | undefined;
  request: RequestSender;
}): Promise<void> {
  return request({ type: "reveal_path_link", project, path }).then(
    () => undefined,
    (error: unknown) => raiseError(error, { title: t("common.revealFailedTitle"), recovery: pathLinkRecovery(error) }),
  );
}
