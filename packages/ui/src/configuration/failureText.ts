import type { Translate } from "../i18n/catalog";
import { ActionableError } from "../errors/actionableError";
import { ConfigurationFileError } from "../platform";

/** What went wrong saving or opening the configuration file, as the failure to report: a file too large or not text
 * worded by its kind, anything else as `failed`, with the system's own text, which cannot be localized, kept apart as
 * the failure's raw `detail`. Its remedy is the file's, or what the system's error names (`classifyError`): saving
 * goes to a writable folder; a file opened that was too large exceeds the read limit, one that was not text is not a configuration file,
 * and one that could not be read at all needs reading. */
export function fileFailure(
  t: Translate,
  err: unknown,
  failed: "configuration.export.couldNotSave" | "configuration.import.couldNotRead",
): ActionableError {
  const saving = failed === "configuration.export.couldNotSave";
  const wrongFileRemedy = saving ? "fileWrite" : "configuration";
  if (err instanceof ConfigurationFileError && err.kind === "too_large") return new ActionableError(t("configuration.file.sizeLimitExceeded"), wrongFileRemedy, { cause: err });
  if (err instanceof ConfigurationFileError && err.kind === "not_text") return new ActionableError(t("configuration.file.notText"), wrongFileRemedy, { cause: err });
  return new ActionableError(t(failed), saving ? "fileWrite" : "fileRead", { cause: err, detail: err instanceof Error ? err.message : String(err) });
}
