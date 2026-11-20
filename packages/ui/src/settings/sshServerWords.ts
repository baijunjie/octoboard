import type { PlainMessageKey } from "../i18n/catalog";
import type { PlatformHost } from "../i18n/platformTerms";

const STEP = {
  general: "settings.remoteAccess.sshServer.step.general",
  sharing: "settings.remoteAccess.sshServer.step.sharing",
  system: "settings.remoteAccess.sshServer.step.system",
  optionalFeatures: "settings.remoteAccess.sshServer.step.optionalFeatures",
  services: "settings.remoteAccess.sshServer.step.services",
} as const satisfies Record<string, PlainMessageKey>;

/** The names the SSH server sentences refer to, each its own message so a translation keeps its own order. Every
 * sentence is given all of them, whichever system's it is: the one that does not use a name ignores it, which keeps
 * the parameters one shape for both systems' messages. */
export const SSH_SERVER_NAMES = {
  remoteLogin: "settings.remoteAccess.remoteLogin.label",
  openSshServer: "settings.remoteAccess.openSsh.label",
  service: "settings.remoteAccess.sshServer.serviceName",
  services: STEP.services,
} as const satisfies Record<string, PlainMessageKey>;

/** The words of the Remote access section's row about the computer's SSH server, by the system's name for it: Remote
 * Login on macOS, OpenSSH Server on Windows, an optional feature added in Settings and run as a service. */
const WORDS = {
  macos: {
    label: "settings.remoteAccess.remoteLogin.label",
    description: "settings.remoteAccess.remoteLogin.accessRequirement",
    off: "settings.remoteAccess.remoteLogin.off",
    unknown: "settings.remoteAccess.remoteLogin.unknown",
    openFailed: "settings.remoteAccess.remoteLogin.openFailed",
    announcedOn: "settings.remoteAccess.remoteLogin.announcement.on",
    announcedOff: "settings.remoteAccess.remoteLogin.announcement.off",
    // The path each sentence sends the user along (`{path}`).
    paths: {
      off: [STEP.general, STEP.sharing],
      unknown: [STEP.general, STEP.sharing],
      openFailed: [STEP.general, STEP.sharing, "settings.remoteAccess.remoteLogin.label"],
    },
  },
  windows: {
    label: "settings.remoteAccess.openSsh.label",
    description: "settings.remoteAccess.openSsh.description",
    off: "settings.remoteAccess.openSsh.off",
    unknown: "settings.remoteAccess.openSsh.unknown",
    openFailed: "settings.remoteAccess.openSsh.openFailed",
    announcedOn: "settings.remoteAccess.openSsh.announcement.on",
    announcedOff: "settings.remoteAccess.openSsh.announcement.off",
    paths: {
      off: [STEP.system, STEP.optionalFeatures],
      unknown: [STEP.system, STEP.optionalFeatures],
      openFailed: [STEP.system, STEP.optionalFeatures],
    },
  },
} as const;

/** The SSH server row's words on `os`: Windows' on Windows, macOS's elsewhere (the section is offered on no other). */
export function sshServerWords(os: PlatformHost["os"]): (typeof WORDS)[keyof typeof WORDS] {
  return WORDS[os === "windows" ? "windows" : "macos"];
}

/** The names of `SSH_SERVER_NAMES`, each made by `name` from the message that holds it (plain text for a sentence read
 * out or searched, a chip for one shown). */
export function sshServerNames<V>(name: (key: PlainMessageKey) => V): Record<keyof typeof SSH_SERVER_NAMES, V> {
  return {
    remoteLogin: name(SSH_SERVER_NAMES.remoteLogin),
    openSshServer: name(SSH_SERVER_NAMES.openSshServer),
    service: name(SSH_SERVER_NAMES.service),
    services: name(SSH_SERVER_NAMES.services),
  };
}
