// The commands, values and website addresses the guides quote, not translated. Most are quoted only here; the website
// writes its own copies of the few it also shows, which have to match: the install command (`installCommand` in
// `apps/web/seo/downloads.ts`) and the download addresses (the redirect pages of `downloadPagePath` there, and
// `releasesUrl`), which `webParity.test.ts` checks. The default port and the website address come from the same
// settings the website and the daemon read (`config/app.json`, and the website's deployment address through
// `site.config.ts`), so the docs cannot quote another port or script address than the site serves.
import appConfig from "../../../config/app.json" with { type: "json" };
import site from "../site.config.ts";

const port = appConfig.defaultServerPort;
const websiteUrl = `${site.origin}${site.websiteBase}`;

export const productName = site.productName;

export const commands = {
  // `--proto '=https' --tlsv1.2` keeps curl on https and a current TLS version while it fetches the script.
  install: `curl --proto '=https' --tlsv1.2 -fsSL ${websiteUrl}install.sh | sh`,
  // The same script with `--service`, which installs, enables and starts a systemd user service.
  installService: `curl --proto '=https' --tlsv1.2 -fsSL ${websiteUrl}install.sh | sh -s -- --service`,
  // How a reader signs in to a server from their own computer, before anything is installed.
  signIn: "ssh <you>@<server>",
  start: "octoboardd start",
  address: "octoboardd address",
  connection: "octoboardd connection",
  update: "octoboardd update",
  restart: "octoboardd restart",
  binDir: "~/.local/bin",
  server: "<server>",
  portOption: "--port <port>",
  defaultPort: String(port),
  linger: 'loginctl enable-linger "$USER"',
  host: "--host",
  sshPort: "--ssh-port",
  connectionScheme: "octoboard+ssh://",
  fingerprintCommand: "ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub",
  authorizedKeys: "~/.ssh/authorized_keys",
  localhost: "localhost",
} as const;

// The port the bridge on a macOS or Linux host listens on, on that host's loopback alone: the one after the default
// port, which a Linux server's own daemon holds.
const bridgePort = port + 1;

// The commands of the guides that reach a host's browser entry through Tailscale or UU Remote instead of SSH. A macOS
// or Linux host serves the browser on its socket, which a bridge on the host's loopback connects a TCP port to; a
// Windows host serves it on its browser port, already a loopback TCP port. Every listener here binds 127.0.0.1 only.
export const tunnel = {
  bridgePort: String(bridgePort),
  bridge: `socat TCP-LISTEN:${bridgePort},bind=127.0.0.1,reuseaddr,fork UNIX-CONNECT:"$HOME/.octoboard/browser.sock"`,
  installSocatMac: "brew install socat",
  installSocatLinux: "sudo apt install socat",
  checkListener: `lsof -nP -iTCP:${port} -sTCP:LISTEN`,
  checkListenerWindows: `netstat -ano | findstr :${port}`,
  serveBridge: `tailscale serve --tcp=${bridgePort} tcp://127.0.0.1:${bridgePort}`,
  serveBrowserPort: `tailscale serve --tcp=${port} tcp://127.0.0.1:${port}`,
  serveBridgeOff: `tailscale serve --tcp=${bridgePort} off`,
  serveBrowserPortOff: `tailscale serve --tcp=${port} off`,
  serveStatus: "tailscale serve status",
  macCli: "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
  linuxOperator: 'sudo tailscale set --operator="$USER"',
  visitorForward: `socat TCP-LISTEN:${port},bind=127.0.0.1,reuseaddr,fork TCP:<host>:<host port>`,
  // The same forward filled in for a Mac host named `my-mac` on the tailnet, whose host port is the bridge's.
  visitorForwardExample: `socat TCP-LISTEN:${port},bind=127.0.0.1,reuseaddr,fork TCP:my-mac:${bridgePort}`,
  visitorForwardWindows: `netsh interface portproxy add v4tov4 listenaddress=127.0.0.1 listenport=${port} connectaddress=<host IP> connectport=<host port>`,
  visitorForwardWindowsOff: `netsh interface portproxy delete v4tov4 listenaddress=127.0.0.1 listenport=${port}`,
  host: "<host>",
  hostIp: "<host IP>",
  hostPort: "<host port>",
  rotate: "octoboardd address --rotate",
  loopback: "127.0.0.1",
  addressStart: `http://127.0.0.1:${port}/`,
} as const;

// The website's fixed download addresses, each leading to the newest file, and the repository's releases page.
export const downloads = {
  macos: `${websiteUrl}download/macos/`,
  windows: `${websiteUrl}download/windows/`,
  "linux-x86_64": `${websiteUrl}download/linux-x86_64/`,
  "linux-aarch64": `${websiteUrl}download/linux-aarch64/`,
  releases: `${site.repositoryUrl.replace(/\/$/, "")}/releases`,
} as const;
