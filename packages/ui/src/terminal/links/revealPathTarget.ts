import { pathLinkRecovery } from "../../errors/errorRecovery";
import { raiseError } from "../../components/alerts/raiseError";
import { t } from "../../i18n/language";
import type { RequestSender } from "../../requests";
import type { PathLinkTarget } from "./pathLinks";

/**
 * What "Open in file manager" on a path link in the terminal does: the daemon shows the path in the host's file manager,
 * a file selected in its folder and a folder opened, whatever the link's click would do with it. A failure is reported
 * under the file manager's failure title, with the remedy of a failed link open.
 */
export function revealPathTarget({ target, project, request }: {
  target: PathLinkTarget;
  /** The session's project, if it has one. */
  project: string | undefined;
  request: RequestSender;
}): Promise<void> {
  return request({ type: "reveal_path_link", project, path: target.path }).then(
    () => undefined,
    (error: unknown) => raiseError(error, { title: t("common.revealFailedTitle"), recovery: pathLinkRecovery(error) }),
  );
}
