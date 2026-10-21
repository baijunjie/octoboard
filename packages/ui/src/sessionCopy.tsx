import React from "react";

import { InlineChip } from "./components/InlineChip";
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
    return project ? <InlineChip size="inherit" dir="auto" wrap>{project.name}</InlineChip> : <>{t("session.location.project")}</>;
  }
  const owner = consoles.get(session.console_id);
  return owner
    ? <Message id="session.location.consoleSession" params={{ console: <InlineChip size="inherit" dir="auto" wrap>{owner.name}</InlineChip> }} />
    : <>{t("session.location.ownConsoleSession")}</>;
}
