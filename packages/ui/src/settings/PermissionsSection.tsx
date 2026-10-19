import { Button } from "@heroui/react";
import React from "react";

import { useT } from "../i18n/react";
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
  // A button unmounts when its permission is granted, and the Accessibility one the moment its
  // request resolves; either can be the control the keyboard was on. The deps are the resolved
  // flags rather than the objects holding them: `status` is a fresh object after every read,
  // including the one on each window focus, which would otherwise run this on window focus
  // forever. `pending` is not among them — a pending button stays mounted, which is why it is
  // marked pending rather than disabled.
  useSectionRefocus([status?.screenRecording, status?.accessibility, refused.screenRecording, refused.accessibility]);

  const line = (message: PlainMessageKey) => <p className="text-sm text-app-muted">{t(message)}</p>;
  // The same line as the `!status` case below, and not redundant with it: with no capability there
  // is nothing to wait for, so saying it straight away spares a browser the blank frame the macOS
  // app spends waiting for its first read.
  if (!macosPermissions) return line("settings.permissions.unavailable");
  // Nothing until the first read is in: the rows would otherwise be preceded by a flash of the
  // line saying there is nothing here to manage, on the very macOS app that has all of it.
  if (!loaded) return <></>;
  // A shell that cannot be asked is not a shell with nothing to report: on macOS the command
  // always answers, so a refused read means the IPC itself is broken (a missing ACL entry, say),
  // and saying "only in the macOS app" there would misdiagnose the one platform that has this.
  if (unreadable) return line("settings.permissions.unreadable");
  if (!status) return line("settings.permissions.unavailable");

  return (
    <>
      <p className="pb-2 text-sm text-app-muted">{t("settings.permissions.lead")}</p>
      {/* `SettingRow` rules between its rows but not above the first, and the lead paragraph above would
          otherwise run straight into a description in the same muted text, here several lines of it.
          The wrapper draws that rule. */}
      <div className="groove-t">
        {PERMISSIONS.map(({ kind, label, description, refused: refusedKey }) => {
          const granted = status[kind];
          // Accessibility is never granted in place: its request only shows the system's dialog and
          // lists Octoboard in System Settings, so that row is expected to stay "not granted" until
          // the user flips the switch there, which the refresh on window focus then picks up.
          let state: string;
          if (granted) state = t("settings.permissions.granted");
          else if (refused[kind]) state = t(refusedKey);
          else state = t("settings.permissions.notGranted");
          return (
            <SettingRow
              key={kind}
              label={t(label)}
              description={
                <>
                  {t(description)}
                  {/* Every sentence here is a line of its own rather than spliced onto the one before
                      it: a separator written in code is a half-width space, which belongs between two
                      English sentences and nowhere near the full stop a language like Chinese ends
                      its own with.

                      The state is also the one thing here that changes without the user acting in
                      this window — they flip the switch in System Settings and come back — so it is
                      announced, from an element that stays mounted whatever it says. `role="status"`
                      reads the whole region out, so the permission's name goes inside it: with two
                      rows changing independently, a bare "Granted." names nothing. The name is glued
                      on with a plain space, which is never rendered and so carries no typography. */}
                  <span role="status" className="block">
                    <span className="sr-only">{t(label)} </span>
                    {state}
                    {/* Screen Recording reaches a process only at launch, so a grant made since is
                        inert until Octoboard restarts. There is deliberately no restart button: a
                        plain `app.restart()` would cut through the quit flow (daemon shutdown, the
                        quit confirmation) in `exit.rs`. The condition is the shell's own two
                        readings, not something remembered here, so the notice survives this section
                        unmounting on a tab switch — which is the normal next thing the user does. */}
                    {kind === "screenRecording" && granted && !status.screenRecordingAtLaunch && (
                      <span className="block">{t("settings.permissions.restart")}</span>
                    )}
                  </span>
                </>
              }
            >
              {!granted &&
                (refused[kind] ? (
                  <Button size="sm" variant="secondary" preventFocusOnPress onPress={() => void openSystemSettings(kind)}>
                    {t("settings.permissions.openSettings")}
                  </Button>
                ) : (
                  <Button size="sm" variant="primary" preventFocusOnPress isPending={pending[kind]} onPress={() => void request(kind)}>
                    {t("settings.permissions.grant")}
                  </Button>
                ))}
            </SettingRow>
          );
        })}
      </div>
    </>
  );
}
