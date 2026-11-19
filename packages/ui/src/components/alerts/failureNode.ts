import type { ReactNode } from "react";

import { daemonMessage } from "../../errors/daemonMessage";
import { useCurrentLanguage } from "../../i18n/react";
import type { ImportFailure } from "../../protocol";
import { useDaemonStore } from "../../store";
import type { DaemonErrorPart } from "./DaemonErrorMessage";
import { codedMessageNode } from "./errorParts";

/** The text of a failure the daemon reported inside a result, as one it answered a request with is
 * worded: in the current language, a record shown by its name. */
export function useFailureText(): (failure: ImportFailure) => string {
  const language = useCurrentLanguage();
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const sessions = useDaemonStore((s) => s.sessions);
  return (failure) => daemonMessage(language, failure.code, failure.params, failure.message, { consoles, projects, sessions });
}

/** The words of such a failure as nodes, where the message sets some of its params as code, those as
 * code. */
export function useFailureNode(): (failure: ImportFailure, part?: DaemonErrorPart) => ReactNode {
  const failureText = useFailureText();
  return (failure, part) => codedMessageNode(failure, failureText(failure), part);
}
