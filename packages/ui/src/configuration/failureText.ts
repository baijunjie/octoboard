import type { ReactNode } from "react";

import { codedMessageNode, type DaemonErrorPart } from "../components/DaemonErrorMessage";
import { daemonMessage } from "../daemonMessage";
import { isolate } from "../i18n/isolate";
import type { Translate } from "../i18n/catalog";
import { useCurrentLanguage } from "../i18n/react";
import { ConfigurationFileError } from "../platform";
import type { ImportFailure } from "../protocol";
import { useDaemonStore } from "../store";

/** Words a failure the daemon reported inside a result, as one it answered a request with is
 * worded: in the current language, a record shown by its name, and where the message sets some of
 * its params as code, those as code. */
export function useFailureNode(): (failure: ImportFailure, part?: DaemonErrorPart) => ReactNode {
  const language = useCurrentLanguage();
  const consoles = useDaemonStore((s) => s.consoles);
  const projects = useDaemonStore((s) => s.projects);
  const sessions = useDaemonStore((s) => s.sessions);
  return (failure, part) =>
    codedMessageNode(failure, daemonMessage(language, failure.code, failure.params, failure.message, { consoles, projects, sessions }), part);
}

/** What went wrong saving or opening the configuration file, worded in the current language: a file
 * too large or not text by its kind, anything else as `failed` with the system's own text, which
 * cannot be localized. */
export function fileFailureText(
  t: Translate,
  err: unknown,
  failed: "configuration.export.failed" | "configuration.import.readFailed",
): string {
  if (err instanceof ConfigurationFileError && err.kind === "too_large") return t("configuration.file.tooLarge");
  if (err instanceof ConfigurationFileError && err.kind === "not_text") return t("configuration.file.notText");
  return t(failed, { detail: isolate(err instanceof Error ? err.message : String(err)) });
}
