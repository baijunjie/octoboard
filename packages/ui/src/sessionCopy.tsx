import React from "react";

import { InlineReference } from "./components/InlineReference";
import type { Translate } from "./i18n/catalog";
import { Message } from "./i18n/react";
import type { Console, Project, Session } from "./protocol";

/** Rich location copy for application titles; native notifications keep `sessionLocation`. */
export function sessionLocationContent(
  t: Translate,
  session: Session,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): React.ReactElement {
  if (session.project_id) {
    const project = projects.get(session.project_id);
    // The project's name stands alone as the whole title, so it is plain text rather than a chip.
    return project ? <span dir="auto">{project.name}</span> : <>{t("session.location.project")}</>;
  }
  const owner = consoles.get(session.console_id);
  return owner
    ? <Message id="session.location.consoleSession" params={{ console: <InlineReference dir="auto" wrap>{owner.name}</InlineReference> }} />
    : <>{t("session.location.ownConsoleSession")}</>;
}
