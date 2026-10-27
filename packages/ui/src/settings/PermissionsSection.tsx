import React, { useEffect, useState } from "react";
import { CircleCheck } from "lucide-react";

import { AttendedStatusAnnouncer } from "../components/AttendedStatusAnnouncer";
import { Button } from "../components/Button";
import { ErrorAlert } from "../components/ErrorAlert";
import { InlineReference } from "../components/InlineReference";
import { StatusBadge } from "../components/StatusBadge";
import { ActionableError } from "../errorRecovery";
import { Message, useT } from "../i18n/react";
import type { PlainMessageKey } from "../i18n/catalog";
import type { MacosPermissionKind } from "../platform";
import { usePlatform } from "../platform/react";
import { reportError } from "../reportError";
import { SettingRow } from "./SettingRow";
import { useMacosPermissions } from "./useMacosPermissions";
import { useSectionRefocus } from "./useSectionRefocus";

const PERMISSIONS = [
  {
    kind: "screenRecording",
    label: "settings.permissions.screenRecording.label",
    systemPane: "settings.permissions.screenRecording.systemPane",
    description: "settings.permissions.screenRecording.description",
    // Worded per permission: macOS really does refuse to prompt a second time for Screen
    // Recording, while Accessibility has no in-place grant to refuse in the first place, so there
    // the pane is simply where it is granted.
    refused: "settings.permissions.screenRecording.refused",
    announced: {
      granted: "settings.permissions.announcement.screenRecording.granted",
      grantedRestart: "settings.permissions.announcement.screenRecording.grantedRestart",
      notGranted: "settings.permissions.announcement.screenRecording.notGranted",
      refused: "settings.permissions.announcement.screenRecording.refused",
    },
  },
  {
    kind: "accessibility",
    label: "settings.permissions.accessibility.label",
    systemPane: "settings.permissions.accessibility.systemPane",
    description: "settings.permissions.accessibility.description",
    refused: "settings.permissions.accessibility.refused",
    announced: {
      granted: "settings.permissions.announcement.accessibility.granted",
      notGranted: "settings.permissions.announcement.accessibility.notGranted",
      refused: "settings.permissions.announcement.accessibility.refused",
    },
  },
] as const satisfies readonly {
  kind: MacosPermissionKind;
  label: PlainMessageKey;
  systemPane: PlainMessageKey;
  description: PlainMessageKey;
  refused: PlainMessageKey;
  announced: {
    granted: PlainMessageKey;
    grantedRestart?: PlainMessageKey;
    notGranted: PlainMessageKey;
    refused: PlainMessageKey;
  };
}[];

/**
 * The macOS permissions the agents in Octoboard's terminals can need. Their requests are
 * attributed to Octoboard, so asking here, with the reason given first, is what keeps a system
 * prompt from appearing out of nowhere mid-task. Everything is absent where there is no macOS
 * shell to ask. For every state a row can be in see `docs/product/macos-privacy-permissions.md`.
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

  const openPane = async (kind: MacosPermissionKind, systemPane: PlainMessageKey) => {
    try {
      await openSystemSettings(kind);
    } catch (cause) {
      const privacy = t("settings.permissions.privacyPane");
      const pane = t(systemPane);
      reportError(
        new ActionableError(t("settings.permissions.openFailed", { privacy, pane }), "none", { cause }),
        undefined,
        undefined,
        <Message
          id="settings.permissions.openFailed"
          params={{
            privacy: <InlineReference dir="auto" wrap>{privacy}</InlineReference>,
            pane: <InlineReference dir="auto" wrap>{pane}</InlineReference>,
          }}
        />,
        t("settings.permissions.openFailedTitle"),
      );
    }
  };

  // Where each permission stands, for its row and for the section's status region.
  const rows = status
    ? PERMISSIONS.map((permission) => {
      const granted = status[permission.kind];
      // Accessibility is never granted in place: its request only shows the system's dialog and
      // lists Octoboard in System Settings, so that row is expected to stay "not granted" until
      // the user flips the switch there, which the refresh on window focus then picks up.
      const stateKey: PlainMessageKey = refused[permission.kind] ? permission.refused : "settings.permissions.notGranted";
      // Screen Recording reaches a process only at launch, so a grant made since is inert until
      // Octoboard restarts. There is deliberately no restart button: a plain `app.restart()` would
      // cut through the quit flow (daemon shutdown, the quit confirmation) in `exit.rs`. The
      // condition is the shell's own two readings, not something remembered here, so the notice
      // survives this section unmounting on a tab switch — which is the normal next thing the
      // user does.
      const restart = permission.kind === "screenRecording" && granted && !status.screenRecordingAtLaunch;
      const { announced } = permission;
      const announcedKey: PlainMessageKey = granted
        ? (restart && "grantedRestart" in announced ? announced.grantedRestart : announced.granted)
        : refused[permission.kind] ? announced.refused : announced.notGranted;
      return { ...permission, granted, stateKey, restart, announcedKey };
    })
    : undefined;
  // One whole sentence per permission and state, each naming its permission as a bare "Granted." names
  // nothing, laid side by side by a catalog template of their own, so no separator is chosen in code.
  const announcement = rows && t("settings.permissions.announcement", {
    screenRecording: t(rows.find((row) => row.kind === "screenRecording")!.announcedKey),
    accessibility: t(rows.find((row) => row.kind === "accessibility")!.announcedKey),
  });

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
          {rows.map(({ kind, label, systemPane, description, granted, stateKey, restart }) => (
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
                      announced from the section's status region (`AttendedStatusAnnouncer`). */}
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
                <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => void openPane(kind, systemPane)}>
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
      {/* Where the permissions stand, from a status region of the section's own: the state is in place as the
          section appears and changes as the user comes back from System Settings, both moments a status written at
          once goes unsaid. The rows show the same states in their descriptions, which stay readable where they stand,
          so a reader going through the rows meets each state twice. */}
      <AttendedStatusAnnouncer text={announcement} />
    </>
  );
}
