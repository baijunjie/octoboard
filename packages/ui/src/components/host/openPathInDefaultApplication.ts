import { pathLinkRecovery } from "../../errors/errorRecovery";
import { raiseDefaultApplicationFailure } from "../alerts/defaultApplicationFailure";
import { t } from "../../i18n/language";
import type { RequestSender } from "../../requests";

/**
 * Has the daemon open `path`, given by a path link wherever the user pressed it (a terminal's link, the header of the
 * viewer the link opened), with the host's default application, by the path link rules, which refuse what the system
 * could launch rather than show. A failure is reported under the default application's failure title, with the remedy of
 * a failed link open.
 */
export function openPathInDefaultApplication({ path, project, request }: {
  /** The link's path, as the link gives it. */
  path: string;
  /** The session's project, if it has one. */
  project: string | undefined;
  request: RequestSender;
}): Promise<void> {
  return request({ type: "open_path_link", project, path }).then(
    () => undefined,
    (error: unknown) => raiseDefaultApplicationFailure(t, error, pathLinkRecovery(error)),
  );
}
