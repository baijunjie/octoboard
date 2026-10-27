import type { AvailableUpdate } from "../../platform";
import type { Scenario, Step } from "../scenario";
import { SAMPLE, shellTerminalOf, snapshotState } from "./builders";

const GROUP = "Updates";

const { console: console_, web, sessions } = SAMPLE;

// What the update scenarios sit over. Sessions that are idle hold a restart up no more than they hold a quit up, so
// the first state is held up by nothing, and the three restart states by a working session, a busy shell, and both.
const idle = sessions.filter(({ status }) => status === "idle");
const busyShell = shellTerminalOf("sh-dev", web, "pnpm dev", 1, { busy: true });
const quiet = snapshotState({ consoles: [console_], projects: [web], sessions: idle });
const sessionsRunning = snapshotState({ consoles: [console_], projects: [web], sessions });
const terminalRunning = snapshotState({ consoles: [console_], projects: [web], sessions: idle, shellTerminals: [busyShell] });
const bothRunning = snapshotState({ consoles: [console_], projects: [web], sessions, shellTerminals: [busyShell] });
const reconnecting = snapshotState({ consoles: [console_], projects: [web], sessions: idle, connectionState: "reconnecting" });

const NOTES = [
  "- Sessions start faster after the window opens.",
  "- A terminal closed while its agent was still writing no longer ends the session.",
  "- The report panel keeps its scroll position when a page is replaced.",
  "- Settings lists which accounts share their conversations.",
  "- Archived sessions whose conversation is gone can be deleted in one step.",
  "- Several smaller fixes to the file viewer and the Git panel.",
].join("\n");

const newer: AvailableUpdate = { version: "0.2.0-beta.2", notes: NOTES };

const FAILED_CHECK = "error sending request for url (https://updates.octoboard.example/beta/latest.json): error trying to connect: dns error: failed to lookup address information";
const FAILED_INSTALL = "failed to download the update archive: the connection closed before the whole file arrived (received 8123456 of 41234567 bytes)";

/** Selects an idle session, so the notice sits under a terminal as it does in use. */
const openSession: Step = (ui) => ui.press(ui.session("Update the dependencies"));
/** Settings on its About section, where the version and update rows are. */
const openSettings: Step = async (ui) => {
  await ui.press(ui.t("rail.settings"));
  await ui.press(ui.t("settings.section.about"));
};
const downloadAndInstall: Step = (ui) => ui.press(ui.t("updates.notice.install"));
const restartNow: Step = (ui) => ui.press(ui.t("updates.notice.restart"));

export const updateScenarios: Scenario[] = [
  {
    id: "updates-notice-available",
    group: GROUP,
    category: "normal",
    title: "Update notice, a newer version found",
    description:
      "A strip along the bottom edge under the terminal, in the accent colour: a download icon, the sentence naming the version, What is new (closed, chevron down), Download and install and an X icon named Later (its tooltip says so) that puts the notice off.",
    state: quiet,
    updater: { check: newer, checked: true },
    steps: [openSession],
  },
  {
    id: "updates-notice-notes-open",
    group: GROUP,
    category: "normal",
    title: "Update notice, release notes open",
    description:
      "What is new is open (chevron up) and the plain-text notes fill a tinted box under the strip's row, keeping their line breaks; the box scrolls when the notes are longer than it is tall, and the terminal above is pushed up by the strip's height.",
    state: quiet,
    updater: { check: newer, checked: true },
    steps: [openSession, (ui) => ui.press(ui.t("updates.notice.notes"))],
  },
  {
    id: "updates-notice-no-notes",
    group: GROUP,
    category: "normal",
    title: "Update notice, a release without notes",
    description: "The strip has no What is new button: only the sentence, Download and install and the Later icon.",
    state: quiet,
    updater: { check: { version: "0.2.0-beta.2" }, checked: true },
    steps: [openSession],
  },
  {
    id: "updates-notice-downloading-unknown-size",
    group: GROUP,
    category: "normal",
    title: "Update notice, downloading, size unknown",
    description: "One row holds the sentence \"Downloading version …\", ending in an ellipsis, and an indeterminate progress bar after it; there is no percentage, no Later icon and no other button, and the strip is no taller than the others.",
    state: quiet,
    updater: { check: newer, checked: true, install: { progress: { downloaded: 3_400_000 }, ends: "pending" } },
    steps: [openSession, downloadAndInstall],
  },
  {
    id: "updates-notice-downloading-percent",
    group: GROUP,
    category: "normal",
    title: "Update notice, downloading, with a percentage",
    description: "One row: the sentence, a determinate bar filled to 45% and the percentage at the row's end, where the unknown-size scenario's bar is indeterminate. Label, track, fill and percentage are all legible on the strip.",
    state: quiet,
    updater: { check: newer, checked: true, install: { progress: { downloaded: 18_555_000, total: 41_234_567 }, ends: "pending" } },
    steps: [openSession, downloadAndInstall],
  },
  {
    id: "updates-notice-installed",
    group: GROUP,
    category: "normal",
    title: "Update notice, installed, waiting for a restart",
    description: "The strip says the version is installed and takes effect after a restart, with Restart now and Later; neither is a dismissal, and nothing restarts by itself.",
    state: quiet,
    updater: { check: newer, checked: true },
    steps: [openSession, downloadAndInstall],
  },
  {
    id: "updates-notice-narrow",
    group: GROUP,
    category: "normal",
    title: "Update notice in a narrow window, notes open",
    description: "The sentence and the buttons wrap onto more than one line and the notes box takes the strip's width. Fixed at 420px because the wrapping only exists below the width where the row fits on one line.",
    width: 420,
    state: quiet,
    updater: { check: newer, checked: true },
    // The sidebar is a drawer at this width, and closed.
    steps: [(ui) => ui.press(ui.t("titleBar.sidebar.show")), openSession, (ui) => ui.press(ui.t("updates.notice.notes"))],
  },
  {
    id: "updates-settings-idle",
    group: GROUP,
    category: "normal",
    title: "About, Updates row before any check",
    description: "Settings opens on About. No check has run yet: the Updates row only says which version is in use, with Check for updates beside it, and the header above shows the same version.",
    state: quiet,
    updater: {},
    steps: [openSettings],
  },
  {
    id: "updates-settings-up-to-date",
    group: GROUP,
    category: "normal",
    title: "About with updates, up to date, Stable channel",
    description: "The whole About section on a platform that can update itself: the icon, the name and \"Version 0.2.0-beta.1\" in the header, then the Updates row saying this version is the latest on the channel with Check for updates beside it, the Update channel row with Stable chosen, the three links, License, Copyright and Third-party licenses with View licenses. Scroll to see all of it.",
    state: quiet,
    preferences: { updateChannel: "stable" },
    updater: { checked: true },
    steps: [openSettings],
  },
  {
    id: "updates-settings-checking",
    group: GROUP,
    category: "normal",
    title: "About, Updates row while a check runs",
    description: "The Updates row says it is checking for updates, and Check for updates shows its pending spinner. The channel is Beta, the default.",
    state: quiet,
    updater: { check: "pending", checked: true },
    steps: [openSettings],
  },
  {
    id: "updates-settings-available",
    group: GROUP,
    category: "normal",
    title: "About, Updates row with a newer version, notice put off",
    description: "The notice was put off with its Later icon, so the update is offered only here: the Updates row names the newer version and the running one, and its primary button is Download and install.",
    state: quiet,
    updater: { check: newer, checked: true },
    steps: [(ui) => ui.press(ui.t("updates.notice.dismiss")), openSettings],
  },
  {
    id: "updates-settings-downloading",
    group: GROUP,
    category: "normal",
    title: "About, Updates row while downloading",
    description: "The Updates row says it is downloading the version and the button shows its pending spinner. The update notice is still there under the dialog's veil.",
    state: quiet,
    updater: { check: newer, checked: true, install: { progress: { downloaded: 3_400_000 }, ends: "pending" } },
    steps: [downloadAndInstall, openSettings],
  },
  {
    id: "updates-settings-installed",
    group: GROUP,
    category: "normal",
    title: "About, Updates row waiting for a restart",
    description: "The notice was hidden with Later; the Updates row still says the version takes effect after a restart and offers Restart now.",
    state: quiet,
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, (ui) => ui.press(ui.t("updates.notice.later")), openSettings],
  },
  {
    id: "updates-settings-installed-unknown-version",
    group: GROUP,
    category: "normal",
    title: "Update installed, its version not known to this window",
    description:
      "The shell said an update was already installed, which this window never saw, so no version is named. The Updates row says that an update is installed and takes effect after a restart, with Restart now; the notice under the dialog's veil says the same in its own sentence, with Restart now and Later.",
    state: quiet,
    updater: { check: "installed", checked: true },
    steps: [openSession, openSettings],
  },
  {
    id: "updates-settings-channel-switched",
    group: GROUP,
    category: "normal",
    title: "About, the channel switched while an update was on offer",
    description:
      "A newer version was on offer on Beta, with its notice; Stable was chosen instead. The offer is withdrawn at once: no notice under the dialog's veil, and the Updates row, no longer offering Download and install, says it is checking for updates on Stable (that check is never answered here). The channel selector reads Stable.",
    state: quiet,
    updater: { check: { beta: newer, stable: "pending" }, checked: true },
    steps: [
      openSession,
      openSettings,
      (ui) => ui.press(ui.t("settings.updates.channel.label")),
      (ui) => ui.press(ui.t("settings.updates.channel.stable")),
    ],
  },
  {
    id: "updates-restart-sessions",
    group: GROUP,
    category: "normal",
    title: "Restart held up by a session",
    description: "Restart now with a working session opens the confirmation titled \"Restart Octoboard\" (the app name is plain text in the title), saying a restart interrupts the sessions, which can each be resumed afterwards; its confirming button reads Restart, not Quit.",
    state: sessionsRunning,
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-terminals",
    group: GROUP,
    category: "normal",
    title: "Restart held up by a terminal",
    description: "Every session is idle and a shell terminal runs something: the confirmation says a restart ends what the terminals run, and mentions no sessions.",
    state: terminalRunning,
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-sessions-and-terminals",
    group: GROUP,
    category: "normal",
    title: "Restart held up by a session and a terminal",
    description: "One confirmation says both are running: the sessions are interrupted and can each be resumed, and what the terminals run is ended.",
    state: bothRunning,
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-notice-with-connection-banner",
    group: GROUP,
    category: "exceptional",
    title: "Update notice above the connection banner",
    description: "The update strip sits directly above the reconnecting banner, each in its own colour, and the terminal ends above both.",
    state: reconnecting,
    updater: { check: newer, checked: true },
    steps: [openSession],
  },
  {
    id: "updates-settings-check-failed",
    group: GROUP,
    category: "exceptional",
    title: "About, Updates row after a check that did not finish",
    description: "An automatic check failed with nothing raised over the window: the Updates row's sentence is only the running version, a warning triangle (named \"Last update check\") stands to the left of Check for updates in the row, and nothing is under the row. Its popover is shown in the next scenario. The mark is announced when an automatic check fails while Settings is open, even after an earlier check was pressed, but not when the latest check was the one pressed in the row, as the error alert dialog does that.",
    state: quiet,
    updater: { check: { error: FAILED_CHECK }, checked: true },
    steps: [openSettings],
  },
  {
    id: "updates-settings-check-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "About, why the last check for updates did not finish",
    description: "The mouse rests on the warning triangle before Check for updates: its popover opens above it with no click and no focus taken, with the title sentence, then \"It is not known whether a newer version exists.\", then \"Press Check for updates to try again.\" with the button's name as a chip and no button of its own.",
    state: quiet,
    updater: { check: { error: FAILED_CHECK }, checked: true },
    steps: [openSettings, (ui) => ui.hoverStatusMark(ui.t("settings.updates.checkFailed.mark"))],
  },
  {
    id: "updates-error-check",
    group: GROUP,
    category: "exceptional",
    title: "Alert, checking for updates failed",
    description: "Check for updates pressed in Settings > About failed: the alert stays until acknowledged, says the check failed and to check the connection, and keeps the updater's own text behind its disclosure.",
    state: quiet,
    updater: { check: { error: FAILED_CHECK } },
    steps: [openSettings, (ui) => ui.press(ui.t("settings.updates.check"))],
  },
  {
    id: "updates-error-install-gone",
    group: GROUP,
    category: "exceptional",
    title: "Alert, the update is no longer there to install",
    description:
      "Download and install was refused because the shell no longer holds the update: the alert says it is no longer available to install and advises checking for updates again, with no raw text behind a disclosure. The notice that offered it is gone from under the alert.",
    state: quiet,
    updater: { check: newer, checked: true, install: { ends: "gone" } },
    steps: [openSession, downloadAndInstall],
  },
  {
    id: "updates-error-install",
    group: GROUP,
    category: "exceptional",
    title: "Alert, downloading the update failed",
    description: "Download and install failed part-way: the alert says so and points at the connection and the disk space, with the updater's own text behind its disclosure. The notice offers the download again behind it.",
    state: quiet,
    updater: { check: newer, checked: true, install: { progress: { downloaded: 8_123_456, total: 41_234_567 }, ends: { error: FAILED_INSTALL } } },
    steps: [openSession, downloadAndInstall],
  },
  {
    id: "updates-restart-failed",
    group: GROUP,
    category: "exceptional",
    title: "Alert, the restart did not happen",
    description: "Restart now, with nothing holding the restart up, goes straight to the shell, which refuses: the error alert dialog is titled \"Could not restart\" and says Octoboard kept running instead of relaunching, and to quit it and open it again; the shell's text is behind Error details, closed, with the Report issue guidance and button of an unexpected failure. The notice that offered the restart stays behind.",
    state: quiet,
    updater: { check: newer, checked: true },
    exitFailure: "the relaunch was refused by the system",
    steps: [openSession, downloadAndInstall, restartNow],
  },
];
