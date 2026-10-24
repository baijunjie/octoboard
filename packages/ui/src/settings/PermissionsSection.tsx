import React, { useEffect, useState } from "react";
import { CircleCheck } from "lucide-react";

import { Button } from "../components/Button";
import { ErrorAlert } from "../components/ErrorAlert";
import { StatusBadge } from "../components/StatusBadge";
import { AFTER_FOCUS_MOVE_DELAY_MS, StatusAnnouncer } from "../components/StatusAnnouncer";
import { useWindowAttended } from "../components/useWindowAttended";
import { Message, useT } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import type { MacosPermissionKind } from "../platform";
import { usePlatform } from "../platform/react";
import { SettingRow } from "./SettingRow";
import { useMacosPermissions } from "./useMacosPermissions";
import { useSectionRefocus } from "./useSectionRefocus";

const PERMISSIONS = [
  {
    kind: "screenRecording",
    label: "settings.permissions.screenRecording.label",
    description: "settings.permissions.screenRecording.description",
    // Worded per permission: macOS really does refuse to prompt a second time for Screen
    // Recording, while Accessibility has no in-place grant to refuse in the first place, so there
    // the pane is simply where it is granted.
    refused: "settings.permissions.screenRecording.refused",
  },
  {
    kind: "accessibility",
    label: "settings.permissions.accessibility.label",
    description: "settings.permissions.accessibility.description",
    refused: "settings.permissions.accessibility.refused",
  },
] as const satisfies readonly {
  kind: MacosPermissionKind;
  label: PlainMessageKey;
  description: PlainMessageKey;
  refused: PlainMessageKey;
}[];

/**
 * The macOS permissions the agents in Octoboard's terminals can need. Their requests are
 * attributed to Octoboard, so asking here, with the reason given first, is what keeps a system
 * prompt from appearing out of nowhere mid-task. Everything is absent where there is no macOS
 * shell to ask. For every state a row can be in see "Permissions" in `docs/product/settings.md`.
 */
export function PermissionsSection(): React.ReactElement {
  const t = useT();
  const { macosPermissions } = usePlatform();
  const { status, loaded, unreadable, refused, pending, request, openSystemSettings } = useMacosPermissions();
  // Whether a read had come in before this render: grants that become unreadable on a later read (the
  // one on each window focus) are announced, while a section that opens on an unreadable first read
  // shows it as part of what it opened with. `ErrorAlert` reads `announce` when it mounts.
  const [settled, setSettled] = useState(false);
  useEffect(() => {
    if (loaded) setSettled(true);
  }, [loaded]);
  // A button unmounts when its permission is granted, and the Accessibility one the moment its
  // request resolves; either can be the control the keyboard was on. The deps are the resolved
  // flags rather than the objects holding them: `status` is a fresh object after every read,
  // including the one on each window focus, which would otherwise run this on window focus
  // forever. `pending` is not among them — a pending button stays mounted, which is why it is
  // marked pending rather than disabled.
  useSectionRefocus([status?.screenRecording, status?.accessibility, refused.screenRecording, refused.accessibility]);

  // Where each permission stands, worded once for its row and for the section's status region.
  const rows = status
    ? PERMISSIONS.map((permission) => {
      const granted = status[permission.kind];
      // Accessibility is never granted in place: its request only shows the system's dialog and
      // lists Octoboard in System Settings, so that row is expected to stay "not granted" until
      // the user flips the switch there, which the refresh on window focus then picks up.
      const stateKey: PlainMessageKey = granted ? "settings.permissions.granted" : refused[permission.kind] ? permission.refused : "settings.permissions.notGranted";
      const state = t(stateKey);
      // Screen Recording reaches a process only at launch, so a grant made since is inert until
      // Octoboard restarts. There is deliberately no restart button: a plain `app.restart()` would
      // cut through the quit flow (daemon shutdown, the quit confirmation) in `exit.rs`. The
      // condition is the shell's own two readings, not something remembered here, so the notice
      // survives this section unmounting on a tab switch — which is the normal next thing the
      // user does.
      const restart = permission.kind === "screenRecording" && granted && !status.screenRecordingAtLaunch ? t("settings.permissions.restart") : undefined;
      return { ...permission, granted, state, stateKey, restart };
    })
    : undefined;
  // Each row's state with the permission's name in front, as a bare "Granted." names nothing, glued
  // with plain spaces: the text is never rendered, so it carries no typography.
  const announcement = rows?.map((row) => [t(row.label), row.state, row.restart].filter(Boolean).join(" ")).join(" ");

  const body = (): React.ReactNode => {
    const line = (message: PlainMessageKey) => <p className="text-sm text-app-muted"><Message id={message} /></p>;
    // The same line as the `!status` case below, and not redundant with it: with no capability there
    // is nothing to wait for, so saying it straight away spares a browser the blank frame the macOS
    // app spends waiting for its first read.
    if (!macosPermissions) return line("settings.permissions.unavailable");
    // Nothing until the first read is in: the rows would otherwise be preceded by a flash of the
    // line saying there is nothing here to manage, on the very macOS app that has all of it.
    if (!loaded) return null;
    // A shell that cannot be asked is not a shell with nothing to report: on macOS the command
    // always answers, so a refused read means the IPC itself is broken (a missing ACL entry, say),
    // and saying "only in the macOS app" there would misdiagnose the one platform that has this.
    if (unreadable) return <ErrorAlert announce={settled} title={t("settings.permissions.unreadable.title")}><Message id="settings.permissions.unreadable" /></ErrorAlert>;
    if (!rows) return line("settings.permissions.unavailable");

    return (
      <>
        <p className="pb-2 text-sm text-app-muted"><Message id="settings.permissions.lead" /></p>
        {/* `SettingRow` rules between its rows but not above the first, and the lead paragraph above would
            otherwise run straight into a description in the same muted text, here several lines of it.
            The wrapper draws that rule. */}
        <div className="groove-t">
          {rows.map(({ kind, label, description, granted, stateKey, restart }) => (
            <SettingRow
              key={kind}
              label={t(label)}
              description={
                <>
                  {t(description)}
                  {/* Every sentence here is a line of its own rather than spliced onto the one before
                      it: a separator written in code is a half-width space, which belongs between two
                      English sentences and nowhere near the full stop a language like Chinese ends
                      its own with. A state still to act on is the row's own description, read where it
                      stands; a granted one is the success badge at the row's end instead. Either is
                      announced from the section's status region (`PermissionsAnnouncer`). */}
                  {!granted && <span className="block"><Message id={stateKey} /></span>}
                  {restart && <span className="block"><Message id="settings.permissions.restart" /></span>}
                </>
              }
            >
              {granted ? (
                <StatusBadge size="lg" color="success" variant="soft" separated={false} className="inline-flex items-center gap-1">
                  <CircleCheck size={16} aria-hidden="true" />
                  {t("settings.permissions.grantedBadge")}
                </StatusBadge>
              ) : (refused[kind] ? (
                <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => void openSystemSettings(kind)}>
                  {t("settings.permissions.openSettings")}
                </Button>
              ) : (
                <Button size="sm" variant="primary" preventFocusOnPress isPending={pending[kind]} onPress={() => void request(kind)}>
                  {t("settings.permissions.grant")}
                </Button>
              ))}
            </SettingRow>
          ))}
        </div>
      </>
    );
  };

  // The region stays mounted whatever the section shows, from the moment it appears.
  return (
    <>
      {body()}
      <PermissionsAnnouncer text={announcement} />
    </>
  );
}

/**
 * Says where the permissions stand, from a status region of the section's own, visually hidden: the
 * rows show the same states as their descriptions, which stay readable where they stand and so are
 * not `aria-hidden` (a reader going through the rows meets each state twice, there and here).
 *
 * The state is in place as the section appears, when the user has just moved focus onto its tab or
 * opened Settings on it, and a status that is there from the region's first render, or written
 * within about 300 ms of a focus move, is not said. It changes, too, as the user comes back from
 * System Settings, which is where the switch is flipped; VoiceOver then speaks the control focus is
 * on, and a status written meanwhile is not said either. So every text, the first included, is
 * written `AFTER_FOCUS_MOVE_DELAY_MS` of attended time after it arrived, the wait started over each
 * time the user comes back; a text that is replaced while it waits is never said.
 */
function PermissionsAnnouncer({ text }: { text: string | undefined }): React.ReactElement {
  const attended = useWindowAttended();
  const [said, setSaid] = useState<string>();
  useEffect(() => {
    if (text === said || !attended) return;
    const timer = setTimeout(() => setSaid(text), AFTER_FOCUS_MOVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [text, said, attended]);
  return <StatusAnnouncer text={said} />;
}
