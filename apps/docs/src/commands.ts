// The commands, values and website addresses the guides quote, not translated. Most are quoted only here; the website
// writes its own copies of the few it also shows, which have to match: the install command (`installCommand` in
// `apps/web/seo/downloads.ts`) and the download addresses (the redirect pages of `downloadPagePath` there, and
// `releasesUrl`), which `webParity.test.ts` checks. The default port and the website address come from the same
// settings the website and the daemon read (`config/app.json`, and the website's deployment address through
// `site.config.ts`), so the docs cannot quote another port or script address than the site serves. The default port is
// the browser port's, 3369 until the owner sets another; the daemon's own port is one the system assigns, which no page quotes.
import appConfig from "../../../config/app.json" with { type: "json" };
import site from "../site.config.ts";

const port = appConfig.defaultBrowserPort;
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
  browserOn: "octoboardd browser on",
  browserOff: "octoboardd browser off",
  connection: "octoboardd connection",
  update: "octoboardd update",
  restart: "octoboardd restart",
  binDir: "~/.local/bin",
  server: "<server>",
  defaultPort: String(port),
  linger: 'loginctl enable-linger "$USER"',
  host: "--host",
  sshPort: "--ssh-port",
  connectionScheme: "octoboard+ssh://",
  fingerprintCommand: "ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub",
  authorizedKeys: "~/.ssh/authorized_keys",
  localhost: "localhost",
} as const;

// The commands of the guides that reach a host's browser entry through Tailscale or UU Remote instead of SSH. Every host
// serves the browser on its browser port, already a loopback TCP port, so only the computer a reader browses from needs a
// relay. Every listener here binds 127.0.0.1 only.
export const tunnel = {
  installSocatMac: "brew install socat",
  installSocatLinux: "sudo apt install socat",
  checkListener: `lsof -nP -iTCP:${port} -sTCP:LISTEN`,
  checkListenerWindows: `netstat -ano | findstr :${port}`,
  serveBrowserPort: `tailscale serve --tcp=${port} tcp://127.0.0.1:${port}`,
  serveBrowserPortOff: `tailscale serve --tcp=${port} off`,
  serveStatus: "tailscale serve status",
  macCli: "/Applications/Tailscale.app/Contents/MacOS/Tailscale",
  linuxOperator: 'sudo tailscale set --operator="$USER"',
  visitorForward: `socat TCP-LISTEN:${port},bind=127.0.0.1,reuseaddr,fork TCP:<host>:<host port>`,
  // The same forward filled in for a Mac host named `my-mac` on the tailnet, whose host port is its browser port.
  visitorForwardExample: `socat TCP-LISTEN:${port},bind=127.0.0.1,reuseaddr,fork TCP:my-mac:${port}`,
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

// Context and continuity group: where a handoff writes the package the receiving session reads. Copied from the
// daemon's `handoff_dir()` (`apps/daemon/src/paths.rs`, `data_dir().join("handoff")`).
export const handoff = {
  packageDir: "~/.octoboard/handoff/",
} as const;

// Accounts and usage group: the folders the Add account form suggests first for each agent. Copied from what the form
// computes: `nextConfigDir` (`packages/ui/src/settings/accounts/agentAccounts.ts`) numbers the provider's usual
// config directory (`providerConfigDir` in `packages/ui/src/agents/providers.ts`, the provider's `default_config_dir`).
export const accountFolders = {
  claude: "~/.claude-1",
  codex: "~/.codex-1",
  grok: "~/.grok-1",
} as const;
