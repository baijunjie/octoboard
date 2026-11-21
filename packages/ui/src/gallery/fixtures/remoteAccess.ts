import type { Step } from "../../fixtures/interact";
import { MACOS_MACHINE, remoteAccessOf, SAMPLE, snapshotState, WINDOWS_MACHINE } from "../../fixtures/builders";
import { MAC_HOST, WINDOWS_HOST } from "../../test/hosts";
import type { Scenario } from "../scenario";

const GROUP = "Settings";

const { console: console_, web, api, sessions } = SAMPLE;

/** The connection details of the Mac as `octoboardd connection` prints them, with the QR code of the text (a real one,
 * one string per row of modules), which the section offers to copy and draws. */
const MAC_CONNECTION = {
  text: "octoboard+ssh://dev@studio-mac.local:22?fingerprint=SHA256:Qm9vcHNjb25zdGFudGZpbmdlcnByaW50Zm9yZ2FsbGVyeQ&name=Studio%20Mac",
  qr: [
    "11111110111000010110001110111000001111111",
    "10000010110010000011111010101111001000001",
    "10111010100101000111011000010100101011101",
    "10111010111010110010000101000010001011101",
    "10111010001000100110000111001001001011101",
    "10000010101100101011110001101101101000001",
    "11111110101010101010101010101010101111111",
    "00000000011110011110111100111101100000000",
    "11001110011011001101010000010000100101111",
    "01000000100111010110011111111110011110111",
    "11101011101100010010000101010100010010001",
    "11000001100110110110011000010110110111001",
    "01010111101111100111111101011101110111010",
    "01100000011001001110010001111010011111011",
    "11000110010000001010100111010100111010101",
    "00101000001010100100110110111101000001000",
    "01101110110110011101011000110101110001000",
    "10000001010100110000000111110000011010011",
    "10111110000101111110011101110100111101111",
    "00100101111011001111010110000111111001010",
    "11110111101101000111011110011000110011000",
    "00010100011010100000000100010000001110101",
    "01010010000110110110000111110000011100001",
    "11001101001100101100111100010110100001000",
    "10010011110010101111011110010100110001111",
    "01110100011001010000010101111000101111111",
    "10011011101100010000110111010010111110111",
    "10100001001100111100110100111111110101010",
    "10000111111111100111011000010110110001000",
    "10010100010000011100010100111000001110101",
    "00100110000000010110111100111110010001101",
    "00011100100010111111010010001100111001000",
    "11101111000110000001111010010101111111101",
    "00000000101000001000000101111001100010001",
    "11111110010111111100110111011111101011101",
    "10000010110001011101110000010110100011010",
    "10111010110001010001010001001111111111001",
    "10111010010011111011011000101001000100111",
    "10111010010111110000100111111110100000101",
    "10000010111100010110111010111110100001011",
    "11111110100010010000111010000100110010010",
  ],
};

/** The same for the Windows PC. */
const WINDOWS_CONNECTION = {
  text: "octoboard+ssh://dev@studio-pc.local:22?fingerprint=SHA256:aGFsbGVyeWZpbmdlcnByaW50Zm9ycHJvZHVjdGlvbg&name=Studio%20PC",
  qr: [
    "11111110010111001011101110001101101111111",
    "10000010100001110010111011110010101000001",
    "10111010011001000010011110110000101011101",
    "10111010100100101001111001110100101011101",
    "10111010000100010110101110011011101011101",
    "10000010111000011100001001010100101000001",
    "11111110101010101010101010101010101111111",
    "00000000001110110010011000111010000000000",
    "11111011100100111100001001011001010101010",
    "10000100000110000001110111001011101111011",
    "01010010110100010110000010111110011100000",
    "11010001010000101001011110011000100001010",
    "01000011001010010010101011100000100100110",
    "10001001010000111011011101101011011110001",
    "11011010000111100000001001110010110000100",
    "01100100001100101011010000000010101011001",
    "11100011110110101110101101100101000000110",
    "11100100110101000101110110000101101011101",
    "01010010101001111100011011111010110011100",
    "10111001010011011110010110001011100111011",
    "00101110001100111000101111100001000010111",
    "01000101001010010111001101001011100111111",
    "11010111111001111000110000110110010001000",
    "11111001010010010000111010001001001101001",
    "00110011101001111000101001110101000000101",
    "01110100010100100001110110001101011110000",
    "11101011010010010010011011011110100000100",
    "01100101100100100001011000010011101011001",
    "10110010011010010010001111101101110000100",
    "10000101000000111001000101011000101111011",
    "10100110100111000110101011110000011100000",
    "10100101100100101001110100110010010011000",
    "10111110010101101001101111100100111110100",
    "00000000101001000111110100001000100011100",
    "11111110110001111100111001110101101011100",
    "10000010010101010110110010111010100011000",
    "10111010110000111000001011010000111110100",
    "10111010110010010111110100011000110101101",
    "10111010101001011010101011010000101110000",
    "10000010110010010010011100100001100111010",
    "11111110111010110001001011100100000010100",
  ],
};

const baseState = { consoles: [console_], projects: [web, api], sessions };

const KEY = "Zm9yLXRoZS1nYWxsZXJ5LW9ubHk";
const MAC_ADDRESSES = ["Devs-MacBook.local", "192.168.1.20", "100.101.102.103", "fd7a:115c:a1e0::1", "2400:4051:8e2:a00:1c3a:5f2b:9d4e:7a10"];
const PORT_ADDRESS = `http://127.0.0.1:3369/?key=${KEY}`;

/** The forward as the daemon words it (`forward.rs`): the bind and the remote end both 127.0.0.1, and the forward failing
 * outright where it cannot bind. */
const portForward = (port: number) => `ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:${port}:127.0.0.1:${port} dev@<server>`;

/** The Mac with remote access and browser access both on and serving: the socket and the browser port. */
const macServing = snapshotState({
  ...baseState,
  ...MACOS_MACHINE,
  remoteAccess: remoteAccessOf({
    allowed: true,
    state: "serving",
    browser_allowed: true,
    browser_state: "serving",
    browser_port: 3369,
    address: PORT_ADDRESS,
    forward_command: portForward(3369),
    server_addresses: MAC_ADDRESSES,
    connection: MAC_CONNECTION,
  }),
});

/** Remote access on and browser access off: only the socket serves, and the browser port is still the user's. */
const macRemoteOnly = snapshotState({
  ...baseState,
  ...MACOS_MACHINE,
  remoteAccess: remoteAccessOf({
    allowed: true,
    state: "serving",
    browser_port: 3369,
    server_addresses: MAC_ADDRESSES,
    connection: MAC_CONNECTION,
  }),
});

/** Both switches off. */
const macOff = snapshotState({ ...baseState, ...MACOS_MACHINE, remoteAccess: remoteAccessOf({ browser_port: 3369 }) });

/** Browser access on, the browser port taken by another program; remote access off. */
const macBrowserPortInUse = snapshotState({
  ...baseState,
  ...MACOS_MACHINE,
  remoteAccess: remoteAccessOf({
    browser_allowed: true,
    browser_state: "failed",
    browser_port: 3369,
    browser_failure: { reason: "port_in_use", detail: "listening on 127.0.0.1:3369: Address already in use (os error 48)" },
  }),
});

/** The Windows PC with both on: the owner pipe for another computer's app, and the browser port. */
const windowsServing = snapshotState({
  ...baseState,
  ...WINDOWS_MACHINE,
  remoteAccess: remoteAccessOf({
    allowed: true,
    state: "serving",
    browser_allowed: true,
    browser_state: "serving",
    browser_port: 3369,
    address: PORT_ADDRESS,
    forward_command: portForward(3369),
    server_addresses: ["STUDIO-PC", "192.168.1.31", "100.101.102.104"],
    connection: WINDOWS_CONNECTION,
  }),
});

/** Both off on Windows. */
const windowsOff = snapshotState({ ...baseState, ...WINDOWS_MACHINE, remoteAccess: remoteAccessOf({ browser_port: 3369 }) });

/** The named pipe, which another computer's app connects through, could not be opened. */
const windowsPipeFailed = snapshotState({
  ...baseState,
  ...WINDOWS_MACHINE,
  remoteAccess: remoteAccessOf({
    allowed: true,
    state: "failed",
    browser_port: 3369,
    failure: { reason: "pipe_failed", detail: "\\\\.\\pipe\\octoboard-owner: Access is denied. (os error 5)" },
  }),
});

const OPEN_REMOTE_ACCESS: Step[] = [(ui) => ui.press(ui.t("rail.settings")), (ui) => ui.press(ui.t("settings.section.access"))];

/** The section with the Browser tab of Other computers chosen, the other of its two ways in. */
const OPEN_BROWSER_TAB: Step[] = [...OPEN_REMOTE_ACCESS, (ui) => ui.pressTab(ui.t("settings.remoteAccess.tabs.browser"))];

export const remoteAccessScenarios: Scenario[] = [
  {
    id: "settings-remote-access-macos",
    group: GROUP,
    category: "normal",
    title: "Access on macOS, both serving",
    description:
      "The Access section of the Mac application with both switches on, under its lead, in three groups. This computer's browser: the Allow browser access switch, on; the Browser port row (Port 3369, Change port); the Local address field, its address with Open in browser and Copy icon buttons at the field's end. Other computers: Remote Login, with its On badge, since the Mac's SSH server is running and the way in logs in through it, above a strip of two tabs, the Octoboard app tab (selected) and Browser (the next scenario); the app tab holds the Allow remote access switch, on, and the connection details, whose short description names the Add remote daemon button and the Connection details field as chips, as one line of text with Copy connection details and, beneath it, their QR code on a white square (it stays black on white in the dark theme). Key: Rotate key, whose description says the key is shared by browser and remote access. The page is long: Other computers and Key are below the fold of a short window.",
    state: macServing,
    host: MAC_HOST,
    desktopApp: true,
    remoteLogin: "on",
    clipboard: { text: "" },
    steps: OPEN_REMOTE_ACCESS,
  },
  {
    id: "settings-remote-access-macos-browser",
    group: GROUP,
    category: "normal",
    title: "Access on macOS, Browser tab",
    description:
      "The Browser tab of Other computers, with browser access on and its port open: Step 1: SSH forward, whose description says to run it on the other computer, to keep it running and what it does, with the command (forwarding 127.0.0.1:3369 to the same on the Mac) and the address in it as a dotted-underlined token on a faint fill, showing the Mac's Bonjour name (Devs-MacBook.local) so the command is ready to copy, and Step 2: Open the address, which names the Local address above (a chip) as the one to open in a browser on the other computer. Remote Login stays above the tab strip.",
    state: macServing,
    host: MAC_HOST,
    desktopApp: true,
    remoteLogin: "on",
    clipboard: { text: "" },
    steps: OPEN_BROWSER_TAB,
  },
  {
    id: "settings-remote-access-macos-remote-only",
    group: GROUP,
    category: "normal",
    title: "Access on macOS, Browser tab while browser access is off",
    description:
      "Remote access on and browser access off: This computer's browser has the Allow browser access switch, off, and the Browser port row (Port 3369, Change port), the user's setting whether or not the daemon serves; no local address. In Other computers the Browser tab, selected, holds one line, Browser access is off, saying to turn on Allow browser access above to open the app in a browser on another computer (the switch's name is a chip), in place of the SSH steps. Key: Rotate key follows, as remote access serves.",
    state: macRemoteOnly,
    host: MAC_HOST,
    desktopApp: true,
    remoteLogin: "on",
    steps: OPEN_BROWSER_TAB,
  },
  {
    id: "settings-remote-access-macos-off",
    group: GROUP,
    category: "normal",
    title: "Access on macOS, both off, Remote Login off",
    description:
      "Both switches off: This computer's browser has the Allow browser access switch, off, and the Browser port row. In Other computers, Remote Login has no On badge but says it is off and where to switch it on, with an Open System Settings button; below it the tab strip, the Octoboard app tab selected and holding only the Allow remote access switch, off, which says that once it is on another computer's app can use the app through an SSH connection to this one. No connection details, and there is no Key group, as nothing serves.",
    state: macOff,
    host: MAC_HOST,
    desktopApp: true,
    remoteLogin: "off",
    steps: OPEN_REMOTE_ACCESS,
  },
  {
    id: "settings-remote-access-windows",
    group: GROUP,
    category: "normal",
    title: "Access on Windows, both serving",
    description:
      "The section of the Windows application with both switches on, laid out as on macOS but in Windows' words: in Other computers OpenSSH Server comes first, which is not running, so its row says where to add and start it, with an Open Settings button in place of the On badge, above the tab strip with the Octoboard app tab selected.",
    state: windowsServing,
    host: WINDOWS_HOST,
    desktopApp: true,
    remoteLogin: "off",
    clipboard: { text: "" },
    steps: OPEN_REMOTE_ACCESS,
  },
  {
    id: "settings-remote-access-forward-address-menu",
    group: GROUP,
    category: "normal",
    title: "Access, choosing the forward's address",
    description:
      "Pressing the address token in the SSH forward command of the Browser tab opens the standard dropdown menu under it, listing this computer's names and addresses (Devs-MacBook.local with its check, then 192.168.1.20, 100.101.102.103 and an IPv6 address, each with a network glyph) and a last item, Enter the address yourself, with a pencil, which puts the <server> placeholder back. Choosing an item closes the menu and the token and the command show it.",
    state: macServing,
    host: MAC_HOST,
    desktopApp: true,
    remoteLogin: "on",
    clipboard: { text: "" },
    steps: [...OPEN_BROWSER_TAB, (ui) => ui.press(ui.t("settings.remoteAccess.forward.address.change", { address: "Devs-MacBook.local" }))],
  },
  {
    id: "settings-remote-access-allow-confirm",
    group: GROUP,
    category: "normal",
    title: "Allow remote access, asking first",
    description:
      "Turning Allow remote access on, in the Octoboard app tab, opens a destructive confirmation, titled Allow remote access?, with three paragraphs: what whoever has the address can do, that the key lasts until it is rotated, and that another computer logs in over SSH. It says nothing of the browser port, which Allow browser access is for. The switch stays off behind it until Allow is chosen.",
    state: windowsOff,
    host: WINDOWS_HOST,
    desktopApp: true,
    remoteLogin: "on",
    steps: [...OPEN_REMOTE_ACCESS, (ui) => ui.press(ui.t("settings.remoteAccess.allow.label"))],
  },
  {
    id: "settings-remote-access-browser-allow-confirm",
    group: GROUP,
    category: "normal",
    title: "Allow browser access, asking first",
    description:
      "Turning Allow browser access on, the first row of This computer's browser, opens a destructive confirmation, titled Allow browser access?, with four paragraphs: what whoever has the address can do, how long the key lasts and where it travels (the browser's history, bookmarks and sync), and the two about the browser port: that another user of the computer can take it while the app is not holding it, and that a browser that reconnects then sends them the key. It does not mention SSH. The switch stays off behind it until Allow is chosen.",
    state: windowsOff,
    host: WINDOWS_HOST,
    desktopApp: true,
    remoteLogin: "on",
    steps: [...OPEN_REMOTE_ACCESS, (ui) => ui.press(ui.t("settings.remoteAccess.browserAccess.label"))],
  },
  {
    id: "settings-remote-access-port-dialog",
    group: GROUP,
    category: "normal",
    title: "Change the browser port",
    description: "Change port, on the Browser port row of This computer's browser, opens a small dialog with one Port field, filled with the current port, a line saying what a valid one is and, when the field is emptied, an example port as its placeholder, and Save. Shown on Windows, where the section and the dialog read the same.",
    state: windowsServing,
    host: WINDOWS_HOST,
    desktopApp: true,
    remoteLogin: "on",
    steps: [...OPEN_REMOTE_ACCESS, (ui) => ui.press(ui.t("settings.remoteAccess.port.change"))],
  },
  {
    id: "settings-remote-access-failed",
    group: GROUP,
    category: "exceptional",
    title: "Remote access on, the named pipe could not be opened",
    description:
      "Allow remote access is on but the daemon could not serve another computer's app: in the Octoboard app tab of Other computers an error alert under the switch, titled Remote access is on, but this computer cannot be reached, says the named pipe could not be opened, with the system's own words (os error 5) behind its Details button. There are no connection details and no Key group; This computer's browser, above, is unaffected (switch off, Browser port). Every other failure reason (the socket, the key) is the same alert with its own sentence.",
    state: windowsPipeFailed,
    host: WINDOWS_HOST,
    desktopApp: true,
    remoteLogin: "on",
    steps: OPEN_REMOTE_ACCESS,
  },
  {
    id: "settings-remote-access-browser-failed",
    group: GROUP,
    category: "exceptional",
    title: "Browser access on, the browser port in use",
    description:
      "Browser access is on but another program holds the browser port: in This computer's browser an error alert under the Allow browser access switch, titled Browser access is on, but the browser port is not open, says the port is in use and to change it, with the system's own words (os error 48) behind its Details button. The Browser port row (Port 3369, Change port) follows, there to choose a free port, and there is no local address. In Other computers the Browser tab, selected, holds one line, The browser port is not open, pointing at the alert under Allow browser access above (its name is a chip), in place of the SSH steps. There is no Key group as nothing serves. Other reasons the port is not open (another failure to listen, the port being the one the app itself uses) are the same alert with their own sentence.",
    state: macBrowserPortInUse,
    host: MAC_HOST,
    desktopApp: true,
    remoteLogin: "on",
    steps: OPEN_BROWSER_TAB,
  },
];
