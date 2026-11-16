// @vitest-environment jsdom
import { afterEach, expect, it } from "vitest";

import { errorAlerts } from "../../components/alerts/errorAlerts";
import { CodedError } from "../../errors/codedError";
import { RemoteFileError } from "../../platform";
import { t } from "../../i18n/language";
import { raiseDownloadFailure } from "./downloadFailure";

afterEach(() => {
  while (errorAlerts.getSnapshot().length > 0) errorAlerts.acknowledge();
});

/** The alert a failure raises: its title, sentence and remedy. */
function raised(failure: unknown, hostOs?: string) {
  raiseDownloadFailure(t, "report.pdf", failure, hostOs);
  const alert = errorAlerts.getSnapshot().at(-1)!;
  return { title: alert.title, message: alert.message?.replace(/[⁨⁩]/g, ""), classification: alert.classification, content: alert.content };
}

it("titles a file that could not be downloaded, and the downloaded file that could not be opened by the step that failed", () => {
  expect(raised(new RemoteFileError("storage", "No space left on device (os error 28)")).title).toBe("Could not download the file");
  expect(raised(new RemoteFileError("incomplete")).title).toBe("Could not download the file");
  expect(raised(new RemoteFileError("open", "no application")).title).toBe("Could not open the downloaded file");
});

it("says that what this computer would run is not downloaded, and offers no remedy", () => {
  const alert = raised(new RemoteFileError("launchable"));
  expect(alert.title).toBe("Could not download the file");
  expect(alert.message).toBe("report.pdf is something this computer would run, so it is not downloaded.");
  expect(alert.classification).toEqual({ kind: "actionable", recovery: "none" });
  // The file's name is a chip in the body; the message keeps it as text.
  expect(alert.content).toBeDefined();
});

it("says a copy that came up short as a file that changed while it was copied", () => {
  const alert = raised(new RemoteFileError("incomplete"));
  expect(alert.message).toBe("This file changed while it was being read.");
  expect(alert.classification).toEqual({ kind: "actionable", recovery: "fileChanging" });
});

it("gives a copy that arrived and that no application opened the default application's remedy", () => {
  expect(raised(new RemoteFileError("open", "no application")).classification).toEqual({ kind: "actionable", recovery: "openDefaultApplication" });
});

it("says a file that changed while it was copied, with the remedy of trying again once it stops changing, and a connection lost on the way has the connection remedy", () => {
  const changed = raised(new CodedError("x", "source_changed", { path: "a" }));
  expect(changed.message).toBe("This file changed while it was being read.");
  expect(changed.classification).toEqual({ kind: "actionable", recovery: "fileChanging" });
  const lost = raised(new CodedError("The connection closed", "connection_closed", {}));
  expect(lost.classification).toEqual({ kind: "actionable", recovery: "connection" });
  expect(lost.title).toBe("Could not download the file");
});

it("keeps the sentence of any other failure the daemon gave, and shows what is unknown as it came", () => {
  const gone = raised(new CodedError("a.txt does not exist", "file_not_found", { path: "a.txt", root: "project" }));
  expect(gone.title).toBe("Could not download the file");
  expect(gone.message).not.toContain("changed");
  // Anything else is unexpected, which offers Report issue.
  expect(raised(new Error("boom")).classification).toEqual({ kind: "unexpected" });
});

it.each([
  ["No space left on device (os error 28)", "diskSpace"],
  ["Permission denied (os error 13)", "access"],
])("gives a copy this computer could not write the remedy of the system's cause (%s)", (detail, recovery) => {
  expect(raised(new RemoteFileError("storage", detail)).classification).toEqual({ kind: "actionable", recovery });
});

it.each([
  ["There is not enough space on the disk. (os error 112)", "diskSpace"],
  ["Access is denied. (os error 5)", "access"],
])("reads a Windows host's own error numbers (%s)", (detail, recovery) => {
  expect(raised(new RemoteFileError("storage", detail), "windows").classification).toEqual({ kind: "actionable", recovery });
  // Not on another system, where the same number is another error.
  expect(raised(new RemoteFileError("storage", detail), "macos").classification).toEqual({ kind: "unexpected" });
});

it("reads a Windows number by Windows's table only, even where the Unix table has the same number", () => {
  // 13 is a denied access on Unix and invalid data on Windows, which has no remedy for it.
  const detail = "The data is invalid. (os error 13)";
  expect(raised(new RemoteFileError("storage", detail), "windows").classification).toEqual({ kind: "unexpected" });
  expect(raised(new RemoteFileError("storage", detail), "macos").classification).toEqual({ kind: "actionable", recovery: "access" });
});

it("gives a file gone from a temporary directory no remedy and one gone from the project the one of finding it again", () => {
  const gone = (root: string) => raised(new CodedError("x does not exist", "file_not_found", { path: "/tmp/x", root })).classification;
  expect(gone("temporary")).toEqual({ kind: "actionable", recovery: "none" });
  expect(gone("project")).toEqual({ kind: "actionable", recovery: "hostPathGone" });
});
