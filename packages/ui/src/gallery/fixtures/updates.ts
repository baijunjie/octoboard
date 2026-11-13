import type { AvailableUpdate } from "../../platform";
import type { Step } from "../../fixtures/interact";
import { PROTOCOL_GENERATION, type DaemonUpdateAvailable, type DaemonUpdateCheck } from "../../protocol";
import type { Scenario } from "../scenario";
import { SAMPLE, daemonUpdateOf, minutesAgo, sessionOf, shellTerminalOf, snapshotState } from "../../fixtures/builders";

const GROUP = "Updates";

const { console: console_, web, sessions } = SAMPLE;

// What the update scenarios sit over. The first state has only idle sessions, which no restart is held up by unless
// the daemon lists them as interrupted; the restart states have a working session, a waiting one and a busy shell.
const idle = sessions.filter(({ status }) => status === "idle");
const busyShell = shellTerminalOf("sh-dev", web, "pnpm dev", 1, { busy: true });
const quiet = snapshotState({ consoles: [console_], projects: [web], sessions: idle });
const bothRunning = snapshotState({ consoles: [console_], projects: [web], sessions, shellTerminals: [busyShell] });
// A restart that cannot carry seven working sessions over, for the list that names four and gathers the rest.
const manySessions = ["Fix the summary layout", "Add idempotency keys", "Migrate the build to Vite", "Speed up the search index", "Rework the result ranking", "Document the export format", "Tidy the settings dialog"]
  .map((title, i) => sessionOf(`s-many-${i}`, console_.id, web.id, title, "working"));
// Two sessions, one working and one idle, for the restart whose answer could not be had: it names every running session.
const workingAndIdle = snapshotState({ consoles: [console_], projects: [web], sessions: sessions.filter(({ id }) => id === "s-web-1" || id === "s-web-2") });
const manyRunning = snapshotState({ consoles: [console_], projects: [web], sessions: manySessions });
const reconnecting = snapshotState({ consoles: [console_], projects: [web], sessions: idle, connectionState: "reconnecting" });

const NOTES = [
  "- Sessions start faster after the window opens.",
  "- A terminal closed while its agent was still writing no longer ends the session.",
  "- The report panel keeps its scroll position when a page is replaced.",
  "- Settings lists which accounts share their conversations.",
  "- Archived sessions whose conversation is gone can be deleted in one step.",
  "- Several smaller fixes to the file viewer and the Git panel.",
].join("\n");

const newer: AvailableUpdate = { version: "0.2.0", notes: NOTES };

// The daemon's own update, for a browser tab whose daemon updates itself: the same offer as the desktop updater's, as the
// daemon's status holds it.
const daemonOffer: DaemonUpdateAvailable = { version: newer.version, notes: NOTES, holder_floor: "0.1.0", channel: "stable" };
const foundOffer: DaemonUpdateCheck = { outcome: "available", at: minutesAgo(25), error: null, detail: null };
// Where a daemon that cannot write to its own folder would have put the new executable.
const NOT_WRITABLE = { reason: "not_writable", path: "/usr/local/bin/octoboardd" } as const;
const browserQuiet = (daemonUpdate: ReturnType<typeof daemonUpdateOf>) => snapshotState({ consoles: [console_], projects: [web], sessions: idle, daemonUpdate });
const REPLACE_FAILED = "rename /usr/local/bin/.octoboardd.update-4242 to /usr/local/bin/octoboardd: Permission denied (os error 13)";

const FAILED_CHECK = "error sending request for url (https://updates.octoboard.example/stable/latest.json): error trying to connect: dns error: failed to lookup address information";

/** Selects an idle session, so the notice sits under a terminal as it does in use. */
const openSession: Step = (ui) => ui.press(ui.session("Update the dependencies"));
/** Settings on its Updates section, where the version, the update controls, the automatic check and the channel are. */
const openSettings: Step = async (ui) => {
  await ui.press(ui.t("rail.settings"));
  await ui.press(ui.t("settings.section.updates"));
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
      "A strip along the bottom edge under the terminal, in the accent colour: a download icon, the sentence naming the version, What is new (an external-link icon, opening the offered version's GitHub release page), Download and install and an X icon named Later (its tooltip says so) that puts the notice off. The link stays available even when the update has no notes. Choose Phone (390px) to see the sentence and controls wrap without an expanded notes box.",
    state: quiet,
    updater: { check: newer, checked: true },
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
    id: "updates-settings-idle",
    group: GROUP,
    category: "normal",
    title: "Updates section before any check, automatic checks off",
    description: "Settings opens on Updates, which sits directly above About. No check has run yet, and none will by itself: the Version row only says which version is in use, with the What is new link button (an external-link icon, opening the running version's release page) and Check for updates beside it, and no last-check line under it. Below are the Check for updates automatically switch, off, whose description says that turning it on lets the application check in the background, and the Update channel row. Choose Phone (390px) to see the controls move beneath their descriptions, end-aligned.",
    state: quiet,
    preferences: { updateAutoCheck: false },
    updater: {},
    steps: [openSettings],
  },
  {
    id: "updates-settings-up-to-date",
    group: GROUP,
    category: "normal",
    title: "Updates section, up to date, automatic checks on, Stable channel",
    description: "The whole Updates section on the desktop application: the Version row saying this version is the latest on the channel, with \"Last checked just now\" under it and What is new and Check for updates beside it, the Check for updates automatically switch on, and the Update channel row with Stable chosen. Choose Phone (390px) to see the Update channel select stretch across the row beneath its description.",
    state: quiet,
    updater: { checked: true },
    steps: [openSettings],
  },
  {
    id: "updates-settings-checking",
    group: GROUP,
    category: "normal",
    title: "Updates section while a check runs",
    description: "The Version row says it is checking for updates, and What is new stays beside Check for updates, which shows its pending spinner. The channel is Stable, the default. Choosing the other channel while an update was on offer withdraws the offer and lands here.",
    state: quiet,
    updater: { check: "pending", checked: true },
    steps: [openSettings],
  },
  {
    id: "updates-settings-available",
    group: GROUP,
    category: "normal",
    title: "Updates section with a newer version, notice put off",
    description: "The notice was put off with its Later icon; the update remains available here: the Version row names the newer version and the running one, and What is new, for the newer version, sits before its primary button, Download and install.",
    state: quiet,
    updater: { check: newer, checked: true },
    steps: [(ui) => ui.press(ui.t("updates.notice.dismiss")), openSettings],
  },
  {
    id: "updates-settings-downloading",
    group: GROUP,
    category: "normal",
    title: "Updates section while downloading",
    description: "The Version row says it is downloading the version and What is new, for that version, sits beside the button, which shows its pending spinner. The update notice is still there under the dialog's veil.",
    state: quiet,
    updater: { check: newer, checked: true, install: { progress: { downloaded: 3_400_000 }, ends: "pending" } },
    steps: [downloadAndInstall, openSettings],
  },
  {
    id: "updates-settings-installed",
    group: GROUP,
    category: "normal",
    title: "Updates section waiting for a restart",
    description: "The notice was hidden with Later; the Version row still says the version takes effect after a restart and offers What is new, for the installed version, and Restart now. The same row when the installed version is not known to this window names no version and has no What is new.",
    state: quiet,
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, (ui) => ui.press(ui.t("updates.notice.later")), openSettings],
  },
  {
    id: "updates-restart-sessions-and-terminals",
    group: GROUP,
    category: "normal",
    title: "Restart held up by sessions and a terminal",
    description: "The confirmation says the restart interrupts the sessions below and ends programs running in terminals, names two sessions (one working, one idle) and ends with \"Everything else keeps running.\". The waiting session is not listed, since it is carried over.",
    state: bothRunning,
    handover: { sessions: ["s-web-1", "s-web-2"], shellTerminals: [busyShell.id] },
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-idle-session",
    group: GROUP,
    category: "normal",
    title: "Restart interrupting an idle session",
    description: "The confirmation opens although nothing is working: it says the restart interrupts the sessions below, names the one idle session and ends with \"Everything else keeps running.\". The working and waiting sessions are carried over and the busy terminal runs on, so none is named.",
    state: bothRunning,
    handover: { sessions: ["s-web-2"] },
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-terminals-only",
    group: GROUP,
    category: "normal",
    title: "Restart held up by a terminal alone",
    description: "No session list: the message says some terminals are still running something and that restarting ends it, followed by \"Everything else keeps running.\". The working and waiting sessions are carried over, so none is named.",
    state: bothRunning,
    handover: { shellTerminals: [busyShell.id] },
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-many-sessions",
    group: GROUP,
    category: "normal",
    title: "Restart interrupting seven sessions",
    description: "The list names four sessions and gathers the other three in its last row, \"and 3 more\", set level with the titles above it (past the status glyph and the agent icon); \"Everything else keeps running.\" follows below the list.",
    state: manyRunning,
    handover: { sessions: manySessions.map(({ id }) => id) },
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-daemon-settings",
    group: GROUP,
    category: "normal",
    title: "Updates section in a browser, up to date",
    description: "The Updates section where there is no desktop application and the daemon updates itself, the way a browser tab sees it: the Version row says this version, the daemon's, is the latest on the channel, \"Last checked 25 minutes ago\" under it and What is new and Check for updates beside it; the Check for updates automatically switch is on and the Update channel row has Stable chosen. It is the same section as the desktop application's, directly above About, and the rows keep working: the switch and the channel change at once.",
    state: browserQuiet(daemonUpdateOf({ last_check: { outcome: "up_to_date", at: minutesAgo(25), error: null, detail: null } })),
    steps: [openSettings],
  },
  {
    id: "updates-daemon-notice-not-writable",
    group: GROUP,
    category: "normal",
    title: "Update notice, the daemon cannot replace itself",
    description: "The strip names the version and says the application cannot replace the executable at its path, so the upgrade is by hand with the install script. It has What is new, Copy install command and the Later icon, and no Download and install. Choose Phone (390px) to see the sentence and the three controls wrap.",
    state: browserQuiet(daemonUpdateOf({ available: daemonOffer, manual: NOT_WRITABLE, last_check: foundOffer })),
    steps: [openSession],
  },
  {
    id: "updates-daemon-settings-not-writable",
    group: GROUP,
    category: "normal",
    title: "Updates section, the daemon cannot replace itself",
    description: "The notice was put off with its Later icon. The Version row says the newer version is available but that the application cannot replace the executable at its path, with What is new beside it and no Download and install button, and Check for updates in its place. Under the row, the Install command label and the one-line command in its own block, which carries a copy button.",
    state: browserQuiet(daemonUpdateOf({ available: daemonOffer, manual: NOT_WRITABLE, last_check: foundOffer })),
    steps: [(ui) => ui.press(ui.t("updates.notice.dismiss")), openSettings],
  },
  {
    id: "updates-daemon-reload",
    group: GROUP,
    category: "normal",
    title: "Reload bar, the daemon now runs another version",
    description: "This page was loaded for version 0.1.0 and the daemon has since restarted into 0.2.0, as another tab's restart does: the strip along the bottom edge carries a refresh icon, the sentence that the application was updated to 0.2.0 and the page is to be reloaded, and Reload. It has no Later icon: the page is out of date until it is reloaded, and nothing reloads by itself.",
    state: browserQuiet(daemonUpdateOf()),
    steps: [
      openSession,
      (ui) => ui.daemonEvents({ type: "hello", generation: PROTOCOL_GENERATION, daemon_version: "0.2.0" }),
    ],
  },
  {
    id: "updates-daemon-updated-toast",
    group: GROUP,
    category: "normal",
    title: "Toast after the daemon updated itself",
    description: "The window opens on a daemon that started by replacing version 0.1.0 with 0.2.0: a toast says \"Updated to version 0.2.0.\" with a What is new link to that version's release page. It is told once per browser for each version, and the toast goes by itself after a few seconds (open the scenario again to see it).",
    state: snapshotState({ consoles: [console_], projects: [web], sessions: idle, daemon: { version: "0.2.0", generation: PROTOCOL_GENERATION }, daemonUpdate: daemonUpdateOf({ updated_from: "0.1.0" }) }),
  },
  {
    id: "updates-daemon-restart-confirmation",
    group: GROUP,
    category: "normal",
    title: "Restart into an installed update, in a browser",
    description: "Restart now of the notice, pressed in a browser tab where no desktop application owns the daemon, asks the same confirmation: it says the restart interrupts the sessions below, names two (one working, one idle) and ends with \"Everything else keeps running.\". The notice that offered the restart is under the dialog's veil.",
    state: snapshotState({
      consoles: [console_],
      projects: [web],
      sessions,
      daemonUpdate: daemonUpdateOf({ phase: "installed", installed: { version: "0.2.0", holder_floor: "0.1.0" }, last_check: foundOffer }),
    }),
    handover: { sessions: ["s-web-1", "s-web-2"] },
    steps: [restartNow],
  },
  {
    id: "updates-restart-uncertain",
    group: GROUP,
    category: "exceptional",
    title: "Restart, what the update keeps running could not be checked",
    description: "The answer is not known, so the message says it could not check which sessions keep running through the update and that the sessions below may be interrupted. Both running sessions are named, working and idle alike, then \"Terminals that cannot be reached right now may end too.\" for the terminals the daemon reported as unreachable; no \"Everything else keeps running.\" line follows.",
    state: workingAndIdle,
    handover: { known: false, sessions: ["s-web-1", "s-web-2"], unreachedShellTerminals: ["sh-unreached"] },
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-unreached-terminals",
    group: GROUP,
    category: "exceptional",
    title: "Restart, terminals that cannot be reached",
    description: "The only reason to ask is terminals of a holder the daemon cannot reach right now: the message alone says they cannot be reached and that restarting may end what they run. No session is named and there is no \"Everything else keeps running.\" line, since nothing says what else is safe.",
    state: bothRunning,
    handover: { unreachedShellTerminals: ["sh-unreached"] },
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-unnamed-and-unreached",
    group: GROUP,
    category: "exceptional",
    title: "Restart, sessions the window cannot name and unreachable terminals",
    description: "Under the message about the interrupted sessions, one named session and a row \"and 2 more\" for two sessions the window holds no title for, then the line \"Terminals that cannot be reached right now may end too.\". \"Everything else keeps running.\" is left out, as the unreached terminals leave that unsaid.",
    state: bothRunning,
    handover: { sessions: ["s-web-1"], unnamedSessions: ["s-unheard-1", "s-unheard-2"], unreachedShellTerminals: ["sh-unreached"] },
    updater: { check: newer, checked: true },
    steps: [downloadAndInstall, restartNow],
  },
  {
    id: "updates-restart-unnamed-sessions-only",
    group: GROUP,
    category: "exceptional",
    title: "Restart, sessions the window cannot name at all",
    description: "No session list: the message itself states the count, \"Restarting interrupts 3 sessions. You can resume each one after the restart.\", followed by \"Everything else keeps running.\".",
    state: bothRunning,
    handover: { unnamedSessions: ["s-unheard-1", "s-unheard-2", "s-unheard-3"] },
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
    title: "Updates section after a check that did not finish",
    description: "An automatic check failed with nothing raised over the window: the Version row's sentence is only the running version, a warning triangle (named \"Last update check\") sits between What is new and Check for updates, beside the button it is about, and nothing is under the row. Its popover is shown in the next scenario. The mark is announced when an automatic check fails while Settings is open, even after an earlier check was pressed, but not when the latest check was the one pressed in the row, as the error alert dialog does that.",
    state: quiet,
    updater: { check: { error: FAILED_CHECK }, checked: true },
    steps: [openSettings],
  },
  {
    id: "updates-settings-check-failed-details",
    group: GROUP,
    category: "exceptional",
    title: "Updates section, why the last check did not finish",
    description: "The mouse rests on the warning triangle between What is new and Check for updates: its popover opens above it with no click and no focus taken, with the title sentence, then \"It is not known whether a newer version exists.\", then \"Press Check for updates to try again.\" with the button's name as a chip and no button of its own.",
    state: quiet,
    updater: { check: { error: FAILED_CHECK }, checked: true },
    steps: [openSettings, (ui) => ui.hoverStatusMark(ui.t("settings.updates.checkFailed.mark"))],
  },
  {
    id: "updates-error-check",
    group: GROUP,
    category: "exceptional",
    title: "Alert, checking for updates failed",
    description: "Check for updates pressed in Settings > Updates failed: the alert stays until acknowledged, says the check failed and to check the connection, and keeps the updater's own text behind its disclosure. A download that fails part-way uses the same alert, pointing at the connection and the disk space.",
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
    id: "updates-daemon-error-install-failed",
    group: GROUP,
    category: "exceptional",
    title: "Alert, the daemon could not put the update in place",
    description: "Download and install was pressed and the daemon's download ended in failure: the alert stays until acknowledged, says the update could not be put in place and the installed daemon was not changed, and keeps the daemon's own text behind its disclosure. It gives no advice to check the connection or the disk, which would not help here. The notice that offered the update is back under it.",
    state: browserQuiet(daemonUpdateOf({ available: daemonOffer, last_check: foundOffer })),
    steps: [
      openSession,
      downloadAndInstall,
      (ui) => ui.daemonEvents({
        type: "daemon_update_status_updated",
        status: daemonUpdateOf({
          available: daemonOffer,
          last_check: foundOffer,
          install: { state: "failed", started_at: Date.now(), finished_at: Date.now(), error: "daemon_update_replace_failed", detail: REPLACE_FAILED },
        }),
      }),
    ],
  },
];
