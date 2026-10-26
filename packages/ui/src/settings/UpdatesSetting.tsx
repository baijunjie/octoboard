import React, { useId, useRef, useState } from "react";

import { Button } from "../components/Button";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { InlineReference } from "../components/InlineReference";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { StatusMark } from "../components/StatusMark";
import { OptionSelect } from "../dialogs/OptionSelect";
import { isolate } from "../i18n/isolate";
import { Message, useT } from "../i18n/react";
import type { UpdateChannel } from "../platform";
import { updateChannel } from "../updates/updateChannel";
import type { UpdateState } from "../updates/updateStore";
import { useUpdates } from "../updates/UpdatesProvider";
import { SettingRow } from "./SettingRow";

/** The sentence under "Updates": what the application's update is up to, in the order of what the user most needs
 * to know — a restart waiting, a download under way, a check running, a newer version, then how the last check
 * ended when it found nothing to fault. A check that did not finish is not in the sentence but in the row's
 * warning mark (`checkFailed`). */
function versionStatus(t: ReturnType<typeof useT>, state: UpdateState): string {
  const version = isolate(state.currentVersion ?? "");
  if (state.phase === "installed") {
    return state.installedVersion
      ? t("settings.updates.version.installed", { installed: isolate(state.installedVersion) })
      : t("settings.updates.version.installedUnknown");
  }
  if (state.phase === "downloading") return t("settings.updates.version.downloading", { available: isolate(state.update?.version ?? "") });
  if (state.phase === "checking") return t("settings.updates.version.checking");
  if (state.update) return t("settings.updates.version.available", { available: isolate(state.update.version), version });
  if (state.lastCheck?.outcome === "upToDate") return t("settings.updates.version.upToDate", { version });
  return t("settings.updates.version.idle", { version });
}

/** Whether the row shows that the last check did not finish: nothing newer is known, and nothing else is under way. */
function checkFailed(state: UpdateState): boolean {
  return state.phase === "idle" && !state.update && state.lastCheck?.outcome === "failed";
}

/** The running version with the control that moves it on — Check for updates, then Download and install once a
 * newer one is found, then Restart now once it is installed — and the channel the application follows. Rendered
 * only where the platform can update itself (`AboutSection`).
 *
 * The sentence is spoken from here, since the update notice's own announcer sits under the Settings modal and is not
 * heard. Once the update's phase has changed while Settings is open, whoever caused it (a press of Check for updates
 * or Download and install here, a check that was already running at open ending, a download started from the notice
 * finishing, an automatic check), every change of the sentence is spoken; what was there when Settings opened is not.
 * The sentence of a check under way is left unspoken, as the button's pending state says it. */
export function UpdatesSetting(): React.ReactElement | null {
  const t = useT();
  const channel = updateChannel.useValue();
  const updates = useUpdates();
  const [engaged, setEngaged] = useState(false);
  // A check pressed here that fails opens the error alert dialog, which owns announcing it: `pressed` is a press
  // waiting for its check to start, `pressedCheck` whether the latest check started from one.
  const [pressed, setPressed] = useState(false);
  const [pressedCheck, setPressedCheck] = useState(false);
  const [phase, setPhase] = useState(updates?.state.phase);
  const markId = useId();
  const checkButton = useRef<HTMLButtonElement>(null);
  if (!updates) return null;
  const { store, state } = updates;
  if (state.phase !== phase) {
    setPhase(state.phase);
    setEngaged(true);
    if (state.phase === "checking") {
      setPressedCheck(pressed);
      setPressed(false);
    }
  }
  const status = versionStatus(t, state);
  const channels: { value: UpdateChannel; label: string }[] = [
    { value: "beta", label: t("settings.updates.channel.beta") },
    { value: "stable", label: t("settings.updates.channel.stable") },
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
      <HandOffOnUnmount from={`[data-status-mark-id="${markId}"]`} to={() => checkButton.current?.focus()} />
      <StatusMark tone="warning" markId={markId} label={t("settings.updates.checkFailed.mark")}>
        <p>{failedTitle}</p>
        <p>{t("settings.updates.checkFailed.description")}</p>
        <p><Message id="settings.updates.checkFailed.remedy" params={{ check: <InlineReference>{t("settings.updates.check")}</InlineReference> }} /></p>
      </StatusMark>
    </>
  );
  const action =
    state.phase === "installed" ? (
      <Button size="sm" variant="primary" onPress={store.restart}>{t("settings.updates.restart")}</Button>
    ) : state.update ? (
      // Pending rather than disabled while a check runs: a disabled button would drop the focus it holds.
      <Button size="sm" variant="primary" isPending={state.phase !== "idle"} onPress={() => void store.install()}>
        {t("settings.updates.install")}
      </Button>
    ) : (
      <Button ref={checkButton} size="sm" variant="secondary" isPending={state.phase === "checking"} onPress={() => { setPressed(true); void store.check("manual"); }}>
        {t("settings.updates.check")}
      </Button>
    );
  return (
    <>
      <SettingRow label={t("settings.updates.version.label")} description={status}>
        <div className="flex items-center gap-2">
          {warning}
          {action}
        </div>
        <StatusAnnouncer text={failed ? (engaged && !pressedCheck ? failedTitle : undefined) : engaged && state.phase !== "checking" ? status : undefined} />
      </SettingRow>
      <SettingRow label={t("settings.updates.channel.label")} description={t("settings.updates.channel.description")}>
        <OptionSelect inline label={t("settings.updates.channel.label")} options={channels} value={channel} onChange={(value) => updateChannel.set(value)} />
      </SettingRow>
    </>
  );
}
