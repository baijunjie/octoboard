import React from "react";

import type { LaunchPair } from "./agents";
import { InlineChip } from "./components/InlineChip";
import type { Translate } from "./i18n/catalog";
import { Message } from "./i18n/react";
import type { Agent, Console, Project, Session } from "./protocol";
import { modelDisplayName } from "./sessionModel";

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

/** Visible model metadata; the row's accessible name retains `sessionModelLabel`. */
export function sessionModelContent(t: Translate, agent: Agent, launch: LaunchPair): React.ReactNode {
  if (launch.model === null && launch.effort === null) return t("session.modelEffort.default");
  return (
    <Message
      id="session.modelEffort"
      params={{
        model: launch.model === null ? t("session.modelEffort.unknownItem") : <InlineChip size="inherit" monospace wrap>{modelDisplayName(t, agent, launch.model)}</InlineChip>,
        effort: launch.effort === null ? t("session.modelEffort.unknownItem") : <InlineChip size="inherit" monospace wrap>{launch.effort}</InlineChip>,
      }}
    />
  );
}
