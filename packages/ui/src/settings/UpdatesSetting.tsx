import React, { useId, useRef, useState } from "react";

import { CopyCommand } from "../components/controls/CopyBox";
import { UpdatedAge } from "../components/text/UpdatedAge";
import { useHandoffAfterRemoval } from "../components/focus/HandoffOnUnmount";
import { InlineReference } from "../i18n/InlineReference";
import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { StatusMark } from "../components/status/StatusMark";
import { ExternalLink } from "../components/text/ExternalLink";
import { OptionSelect } from "../components/form/OptionSelect";
import { isolate } from "../i18n/isolate";
import { formatRelativeTime, isJustNow } from "../format/relativeTime";
import { useNow } from "../format/useNow";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { UpdateChannel } from "../platform";
import type { HostFacts } from "../platform/host";
import { releaseUrl } from "../components/text/releaseUrl";
import { useHost } from "../platform/react";
import { installedMessageKey } from "../updates/installedMessageKey";
import { reloadPage } from "../platform/reloadPage";
import { restartPress } from "../updates/restartPress";
import type { UpdateState } from "../updates/updateStore";
import { useUpdates } from "../updates/UpdatesProvider";
import { TitledControl } from "../components/hint/TitledControl";
import { SettingRow, SwitchSettingRow } from "./SettingRow";
import { Button } from "../primitives";

/** The sentence under the row's name: what the application's update is up to, in the order of what the user most needs
 * to know — a restart waiting (or held back by an agent's update, `UpdateState.restartBlocked`), a download under way, a
 * check running, a newer version, then how the last check ended when it found nothing to fault. A check that did not
 * finish is not in the sentence but in the row's warning mark (`checkFailed`). It comes in two forms of the same
 * message: `text`, which the announcer speaks, and `shown`, the row's own, where the file the daemon cannot replace is
 * a chip (`<Message>`) and not an isolated string. */
function versionStatus(t: ReturnType<typeof useT>, state: UpdateState, updater: HostFacts["updater"]): { text: string; shown: React.ReactNode } {
  const version = isolate(state.currentVersion ?? "");
  if (state.reloadNeeded) return { text: t("settings.updates.version.reload", { version }), shown: <Message id="settings.updates.version.reload" params={{ version }} /> };
  if (state.phase === "installed") {
    const key = installedMessageKey(updater, "settings", !!state.installedVersion, state.restartBlocked !== undefined);
    const params = { installed: isolate(state.installedVersion ?? "") };
    return { text: t(key, params), shown: <Message id={key} params={params} /> };
  }
  if (state.phase === "downloading") {
    const params = { available: isolate(state.update?.version ?? "") };
    return { text: t("settings.updates.version.downloading", params), shown: <Message id="settings.updates.version.downloading" params={params} /> };
  }
  if (state.phase === "checking") return { text: t("settings.updates.version.checking"), shown: <Message id="settings.updates.version.checking" /> };
  if (state.update && state.manual) {
    const available = isolate(state.update.version);
    return {
      text: t("settings.updates.version.manual", { available, path: isolate(state.manual.path) }),
      shown: <Message id="settings.updates.version.manual" params={{ available, path: <InlineReference monospace wrap>{state.manual.path}</InlineReference> }} />,
    };
  }
  if (state.update) {
    const params = { available: isolate(state.update.version), version };
    return { text: t("settings.updates.version.available", params), shown: <Message id="settings.updates.version.available" params={params} /> };
  }
  if (state.lastCheck?.outcome === "upToDate") return { text: t("settings.updates.version.upToDate", { version }), shown: <Message id="settings.updates.version.upToDate" params={{ version }} /> };
  return { text: t("settings.updates.version.idle", { version }), shown: <Message id="settings.updates.version.idle" params={{ version }} /> };
}

/** Whether the row shows that the last check did not finish: nothing newer is known, and nothing else is under way. */
function checkFailed(state: UpdateState): boolean {
  return state.phase === "idle" && !state.update && state.lastCheck?.outcome === "failed";
}

/** What the Updates section holds: the running version with the control that moves it on — Check for updates, then
 * Download and install once a newer one is found, then Restart now once it is installed (unavailable, with the reason,
 * while an agent's update runs and the daemon would refuse the restart) — beside What is new, the release notes on
 * GitHub of the version the sentence is about and when the last check ended; the switch for the automatic checks; and
 * the channel the application follows. The same for the desktop application and for a daemon that updates itself,
 * which is what the store behind it hides. A daemon that cannot replace its own executable offers no download: the row
 * says so and gives the install command to run by hand below it, with its copy button; a page whose daemon now
 * runs another version offers Reload. The section is listed only where something updates (`SettingsDialog`).
 *
 * The sentence is spoken from here, since the update notice's own announcer sits under the Settings modal and is not
 * heard. Once the update's phase has changed while Settings is open, or the restart of an installed update has been
 * held back by an agent's update or let go, whoever caused it (a press of Check for updates or Download and install
 * here, a check that was already running at open ending, a download started from the notice finishing, an automatic
 * check, an agent's update starting or ending), every change of the sentence is spoken; what was there when Settings opened is not.
 * The sentence of a check under way is left unspoken, as the button's pending state says it. */
export function UpdatesSetting(): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const now = useNow();
  const host = useHost();
  const updates = useUpdates();
  const [engaged, setEngaged] = useState(false);
  // A check pressed here that fails opens the error alert dialog, which owns announcing it: `pressed` is a press
  // waiting for its check to start, `pressedCheck` whether the latest check started from one.
  const [pressed, setPressed] = useState(false);
  const [pressedCheck, setPressedCheck] = useState(false);
  const [phase, setPhase] = useState(updates?.state.phase);
  const [restartBlocked, setRestartBlocked] = useState(updates?.state.restartBlocked);
  const markId = useId();
  const notesId = useId();
  const commandId = useId();
  const actionButton = useRef<HTMLButtonElement>(null);
  // The mark and the notes link each hand focus they held to the row's action as they go.
  const handoffFrom = useHandoffAfterRemoval(() => actionButton.current);
  if (!updates) return <></>;
  const { store, state } = updates;
  // The sentence changes with the restart held back or let go, which is spoken like a change of phase.
  if (state.restartBlocked !== restartBlocked) {
    setRestartBlocked(state.restartBlocked);
    setEngaged(true);
  }
  if (state.phase !== phase) {
    setPhase(state.phase);
    setEngaged(true);
    if (state.phase === "checking") {
      setPressedCheck(pressed);
      setPressed(false);
    }
  }
  const { text: status, shown: shownStatus } = versionStatus(t, state, host.updater);
  const channels: { value: UpdateChannel; label: string }[] = [
    { value: "stable", label: t("settings.updates.channel.stable") },
    { value: "beta", label: t("settings.updates.channel.beta") },
  ];
  // Pressing Check for updates again is the way to retry, so the mark has no button of its own. A failed
  // automatic check that ends while Settings is open is spoken here, as the mark appearing says nothing by
  // itself; one pressed here is spoken by the error alert dialog it also opens. The sentence of the row is left
  // unspoken either way.
  const failed = checkFailed(state);
  const failedTitle = t("settings.updates.checkFailed.title");
  const warning = failed && (
    <>
      {/* The mark goes as a check starts; focus it or its popover held goes to Check for updates. */}
      {handoffFrom(`[data-status-mark-id="${markId}"]`)}
      <StatusMark tone="warning" markId={markId} label={t("settings.updates.checkFailed.mark")}>
        <p>{failedTitle}</p>
        <p>{t("settings.updates.checkFailed.description")}</p>
        <p><Message id="settings.updates.checkFailed.remedy" params={{ check: <InlineReference>{t("settings.updates.check")}</InlineReference> }} /></p>
      </StatusMark>
    </>
  );
  // The version the row's sentence is about: the one installed or offered once there is one, else the running one.
  // An update installed without a known version leaves the link out rather than pointing at the running version.
  const notesVersion = state.phase === "installed" ? state.installedVersion : state.update?.version ?? state.currentVersion;
  // The link goes when the row turns to an installed update of no known version; focus held on it goes to the action.
  const notes = notesVersion && (
    <>
      {/* Closing Settings with focus here leaves the restore to the dialog. */}
      {handoffFrom(`[data-release-notes-id="${notesId}"]`)}
      <ExternalLink data-release-notes-id={notesId} href={releaseUrl(notesVersion)} appearance="button" size="sm">{t("settings.updates.notes")}</ExternalLink>
    </>
  );
  const restart = restartPress(t, state, store);
  const action =
    state.reloadNeeded ? (
      <Button ref={actionButton} size="sm" variant="primary" onPress={reloadPage}>{t("settings.updates.reload")}</Button>
    ) : state.phase === "installed" ? (
      // The title stays put and only goes inert, so the button is not remounted (losing focus) as it toggles.
      <TitledControl title={restart.unavailable ?? t("updates.restartBlocked")} isDisabled={restart.unavailable === undefined}>
        <Button ref={actionButton} size="sm" variant="primary" label={t("settings.updates.restart")} pending={state.restarting} unavailable={restart.unavailable} onPress={restart.onPress}>{t("settings.updates.restart")}</Button>
      </TitledControl>
    ) : state.update && !state.manual ? (
      // Pending rather than disabled while a check runs: a disabled button would drop the focus it holds.
      <Button ref={actionButton} size="sm" variant="primary" pending={state.phase !== "idle"} onPress={() => void store.install()}>
        {t("settings.updates.install")}
      </Button>
    ) : (
      <Button ref={actionButton} size="sm" variant="secondary" pending={state.phase === "checking" || state.checkAsked !== undefined} onPress={() => { setPressed(true); void store.check("manual"); }}>
        {t("settings.updates.check")}
      </Button>
    );
  const manual = state.manual && state.update && !state.reloadNeeded;
  const lastCheck = state.lastCheck && state.phase !== "checking" && !failed ? state.lastCheck : undefined;
  // A check the daemon let pass (it looks again at most every 30 seconds) changes nothing on the row, so the press is
  // answered by saying that the last check stands.
  const lastCheckSentence = state.lastCheck && (isJustNow(state.lastCheck.at, now) ? t("settings.updates.lastCheckNow") : t("settings.updates.lastCheck", { age: formatRelativeTime(language, state.lastCheck.at, now) }));
  const announced = state.checkAsked === "unchanged" ? lastCheckSentence : failed ? (engaged && !pressedCheck ? failedTitle : undefined) : engaged && state.phase !== "checking" ? status : undefined;
  return (
    <>
      <SettingRow
        searchId="updates.version"
        label={t("settings.updates.status.label")}
        description={
          <>
            <span className="block">{shownStatus}</span>
            {lastCheck && <span className="block"><UpdatedAge at={lastCheck.at} copy={{ now: "settings.updates.lastCheckNow", age: "settings.updates.lastCheck" }} /></span>}
          </>
        }
        below={
          <>
            {manual && state.manual && (
              <div role="group" aria-labelledby={commandId} className="flex flex-col gap-1.5">
                <p id={commandId} className="text-sm text-app-muted">{t("settings.updates.manual.command")}</p>
                <CopyCommand text={state.manual.command} copyText={t("updates.notice.copyCommand")} failedText={t("updates.notice.copyCommandFailed")} />
              </div>
            )}
          </>
        }
      >
        <div className="flex items-center gap-2">
          {notes}
          {warning}
          {action}
        </div>
        <StatusAnnouncer text={announced} />
      </SettingRow>
      <SwitchSettingRow
        searchId="updates.autoCheck"
        label={t("settings.updates.autoCheck.label")}
        description={state.autoCheck ? <Message id="settings.updates.autoCheck.description.on" /> : <Message id="settings.updates.autoCheck.description.off" />}
        isSelected={state.autoCheck}
        onChange={(value) => store.setAutoCheck(value)}
      />
      <SettingRow fill searchId="updates.channel" label={t("settings.updates.channel.label")} description={t("settings.updates.channel.description")}>
        <OptionSelect inline label={t("settings.updates.channel.label")} options={channels} value={state.channel} onChange={(value) => void store.setChannel(value)} />
      </SettingRow>
    </>
  );
}
