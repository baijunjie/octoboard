import React, { useState } from "react";

import { Button } from "../components/Button";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { OptionSelect } from "../dialogs/OptionSelect";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";
import type { UpdateChannel } from "../platform";
import { updateChannel } from "../updates/updateChannel";
import type { UpdateState } from "../updates/updateStore";
import { useUpdates } from "../updates/UpdatesProvider";
import { SettingRow } from "./SettingRow";

/** The sentence under "Updates": what the application's update is up to, in the order of what the user most needs
 * to know — a restart waiting, a download under way, a check running, a newer version, then how the last check
 * ended. */
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
  if (state.lastCheck?.outcome === "failed") return t("settings.updates.version.failed", { version });
  if (state.lastCheck?.outcome === "upToDate") return t("settings.updates.version.upToDate", { version });
  return t("settings.updates.version.idle", { version });
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
  const [phase, setPhase] = useState(updates?.state.phase);
  if (!updates) return null;
  const { store, state } = updates;
  if (state.phase !== phase) {
    setPhase(state.phase);
    setEngaged(true);
  }
  const status = versionStatus(t, state);
  const channels: { value: UpdateChannel; label: string }[] = [
    { value: "beta", label: t("settings.updates.channel.beta") },
    { value: "stable", label: t("settings.updates.channel.stable") },
  ];
  const action =
    state.phase === "installed" ? (
      <Button size="sm" variant="primary" onPress={store.restart}>{t("settings.updates.restart")}</Button>
    ) : state.update ? (
      // Pending rather than disabled while a check runs: a disabled button would drop the focus it holds.
      <Button size="sm" variant="primary" isPending={state.phase !== "idle"} onPress={() => void store.install()}>
        {t("settings.updates.install")}
      </Button>
    ) : (
      <Button size="sm" variant="secondary" isPending={state.phase === "checking"} onPress={() => void store.check("manual")}>
        {t("settings.updates.check")}
      </Button>
    );
  return (
    <>
      <SettingRow label={t("settings.updates.version.label")} description={status}>
        {action}
        <StatusAnnouncer text={engaged && state.phase !== "checking" ? status : undefined} />
      </SettingRow>
      <SettingRow label={t("settings.updates.channel.label")} description={t("settings.updates.channel.description")}>
        <OptionSelect inline label={t("settings.updates.channel.label")} options={channels} value={channel} onChange={(value) => updateChannel.set(value)} />
      </SettingRow>
    </>
  );
}
