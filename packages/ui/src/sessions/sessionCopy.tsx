import React from "react";

import { InlineReference } from "../i18n/InlineReference";
import type { Translate } from "../i18n/catalog";
import { Message } from "../i18n/react";
import type { Console, Project, Session } from "../protocol";
import { sessionLocationPhrase } from "./sessionLabel";

/** Rich location copy for application titles; native notifications keep `sessionLocation`. */
export function sessionLocationContent(
  t: Translate,
  session: Session,
  consoles: Map<string, Console>,
  projects: Map<string, Project>,
): React.ReactElement {
  const phrase = sessionLocationPhrase(session, consoles, projects);
  // The project's name stands alone as the whole title, so it is plain text rather than a chip.
  if ("name" in phrase) return <span dir="auto">{phrase.name}</span>;
  if ("console" in phrase) return <Message id={phrase.key} params={{ console: <InlineReference dir="auto" wrap>{phrase.console}</InlineReference> }} />;
  return <>{t(phrase.key)}</>;
}
