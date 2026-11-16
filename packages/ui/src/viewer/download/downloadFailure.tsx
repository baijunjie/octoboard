import { raiseError } from "../../components/alerts/raiseError";
import { ActionableError } from "../../errors/actionableError";
import { CodedError } from "../../errors/codedError";
import { pathLinkRecovery, type ErrorRecovery } from "../../errors/errorRecovery";
import type { Translate } from "../../i18n/catalog";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { Message } from "../../i18n/react";
import { RemoteFileError } from "../../platform";

/** What the system's own error numbers on Windows ask for: a full disk and a denied or write-protected folder. Those of
 * the other systems are read off the text of the failure as for any failure (`classifyError`); a Windows number
 * would mean something else there (5 is an I/O error on Unix), so these are taken only on a Windows host, and the
 * Unix table is not consulted for a Windows number. */
const WINDOWS_ERRNO: Record<string, ErrorRecovery> = { "112": "diskSpace", "39": "diskSpace", "5": "access", "19": "access" };

/**
 * Raises the error alert for a Download and open that failed with `failure`, titled by the step that failed: the file
 * could not be downloaded, or, once all of it had arrived, the downloaded file could not be opened.
 *
 * The daemon's own failures keep their sentences and remedies (a file that is gone, that may not be read, that leads
 * outside what a link may open); a file that changed while it was copied says so, a connection lost on the way has the
 * connection remedy. The daemon's failures to read the file have the remedies of a failed click on a path link (a file
 * gone from the project is found in its files, one gone from a temporary directory has nowhere to be found). The
 * shell's kinds: what this computer would run says so and has no remedy; a copy that came up short says the file
 * changed, as a changed one does; a copy that could not be written has the disk or access remedy by what the system
 * said (read off its text, a Windows host's own numbers included, `hostOs` being the host of this computer's shell); a
 * copy that arrived but that no application opened has the default application's remedy. Anything else is unexpected,
 * its text under Error details.
 */
export function raiseDownloadFailure(t: Translate, name: string, failure: unknown, hostOs?: string): void {
  const title = t("viewer.download.failedTitle");
  const sourceChanged = () => raiseError(new ActionableError(t("viewer.error.sourceChanged"), "fileChanging", { cause: failure }), { title });
  if (failure instanceof RemoteFileError) {
    switch (failure.kind) {
      case "launchable":
        // The name is a chip in the alert's body; the message keeps it as text for the report.
        return raiseError(new ActionableError(t("viewer.download.launchable", { name: isolate(name) }), "none", { cause: failure }), {
          title,
          content: <Message id="viewer.download.launchable" params={{ name: <InlineReference monospace dir="ltr" wrap>{name}</InlineReference> }} />,
        });
      case "open":
        return raiseError(new Error(failure.message), { title: t("viewer.download.openFailedTitle"), recovery: "openDefaultApplication" });
      case "storage": {
        // On a Windows host the number is Windows's own: worded as "(error N)" the Unix table `classifyError` reads
        // off "(os error N)" cannot take it for another error (13 is a denied access there, invalid data here).
        if (hostOs === "windows") {
          const number = /\(os error (\d+)\)/.exec(failure.message)?.[1];
          const message = failure.message.replace(/\(os error (\d+)\)/, "(error $1)");
          return raiseError(new Error(message), { title, recovery: number === undefined ? undefined : WINDOWS_ERRNO[number] });
        }
        return raiseError(new Error(failure.message), { title });
      }
      // The parts that arrived are not the whole file: it changed while it was copied.
      case "incomplete":
        return sourceChanged();
    }
  }
  if (failure instanceof CodedError && failure.code === "source_changed") {
    return sourceChanged();
  }
  if (failure instanceof CodedError && failure.code === "connection_closed") {
    return raiseError(new ActionableError(failure.message, "connection", { cause: failure }), { title });
  }
  raiseError(failure, { title, recovery: pathLinkRecovery(failure) });
}
