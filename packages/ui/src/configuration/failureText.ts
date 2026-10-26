import type { ReactNode } from "react";

import type { DaemonErrorPart } from "../components/DaemonErrorMessage";
import { codedMessageNode } from "../components/errorParts";
import { daemonMessage } from "../daemonMessage";
import { ActionableError } from "../errorRecovery";
import type { Translate } from "../i18n/catalog";
import { useCurrentLanguage } from "../i18n/react";
import { ConfigurationFileError } from "../platform";
import type { ImportFailure } from "../protocol";
import { useDaemonStore } from "../store";

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

/** What went wrong saving or opening the configuration file, as the failure to report: a file too large or not text
 * worded by its kind, anything else as `failed`, with the system's own text, which cannot be localized, kept apart as
 * the failure's raw `detail`. Its remedy is the file's, or what the system's error names (`classifyError`). */
export function fileFailure(
  t: Translate,
  err: unknown,
  failed: "configuration.export.couldNotSave" | "configuration.import.couldNotRead",
): ActionableError {
  if (err instanceof ConfigurationFileError && err.kind === "too_large") return new ActionableError(t("configuration.file.tooLarge"), "file", { cause: err });
  if (err instanceof ConfigurationFileError && err.kind === "not_text") return new ActionableError(t("configuration.file.notText"), "file", { cause: err });
  return new ActionableError(t(failed), "file", { cause: err, detail: err instanceof Error ? err.message : String(err) });
}
