import { linuxCommandParts } from "./downloads.ts";

// Slot names in the website catalogs and the application labels they quote.
export const remoteAppLabels = {
  remoteLabelDevices: "remoteDevices.label",
  remoteLabelMenuDevices: "menu.remoteDevices",
  remoteLabelWindowsDevices: "jumpList.remoteDevices",
  remoteLabelAdd: "remoteDaemons.add",
  remoteLabelDetails: "remoteDaemons.form.details",
  remoteLabelPaste: "common.paste",
  remoteLabelPassword: "remoteDaemons.form.auth.password",
  remoteLabelKeyFile: "remoteDaemons.form.auth.key",
  remoteLabelSave: "common.save",
  remoteLabelConnect: "remoteDaemons.connect",
  remoteLabelSettings: "settings.title",
  remoteLabelAccess: "settings.section.remoteAccess",
  remoteLabelLogin: "settings.remoteAccess.remoteLogin.label",
  remoteLabelOpenSsh: "settings.remoteAccess.openSsh.label",
  remoteLabelAllow: "settings.remoteAccess.allow.label",
  remoteLabelAppTab: "settings.remoteAccess.tabs.app",
  remoteLabelBrowserTab: "settings.remoteAccess.tabs.browser",
} as const;

export const remoteCode = {
  host: "--host",
  port: "--ssh-port",
  connectionScheme: "octoboard+ssh://",
  addressCommand: linuxCommandParts.address,
  binDir: linuxCommandParts.binDir,
  start: linuxCommandParts.start,
  server: linuxCommandParts.server,
  fingerprintCommand: "ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub",
  authorizedKeys: "~/.ssh/authorized_keys",
  localhost: "localhost",
} as const;
