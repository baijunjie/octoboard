import type { PlainMessageKey } from "../i18n/catalog";
import type { PlatformHost } from "../i18n/platformTerms";

/** The words of the Remote access section's row about the computer's SSH server, by the system's name for it: Remote
 * Login on macOS, OpenSSH Server on Windows, an optional feature added in Settings and run as a service. */
const WORDS = {
  macos: {
    label: "settings.remoteAccess.remoteLogin.label",
    description: "settings.remoteAccess.remoteLogin.description",
    off: "settings.remoteAccess.remoteLogin.off",
    unknown: "settings.remoteAccess.remoteLogin.unknown",
    openFailed: "settings.remoteAccess.remoteLogin.openFailed",
    announcedOn: "settings.remoteAccess.remoteLogin.announcement.on",
    announcedOff: "settings.remoteAccess.remoteLogin.announcement.off",
  },
  windows: {
    label: "settings.remoteAccess.openSsh.label",
    description: "settings.remoteAccess.openSsh.description",
    off: "settings.remoteAccess.openSsh.off",
    unknown: "settings.remoteAccess.openSsh.unknown",
    openFailed: "settings.remoteAccess.openSsh.openFailed",
    announcedOn: "settings.remoteAccess.openSsh.announcement.on",
    announcedOff: "settings.remoteAccess.openSsh.announcement.off",
  },
} as const satisfies Record<string, Record<string, PlainMessageKey>>;

/** The SSH server row's words on `os`: Windows' on Windows, macOS's elsewhere (the section is offered on no other). */
export function sshServerWords(os: PlatformHost["os"]): (typeof WORDS)[keyof typeof WORDS] {
  return WORDS[os === "windows" ? "windows" : "macos"];
}
