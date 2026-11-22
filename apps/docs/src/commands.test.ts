import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";
import appConfig from "../../../config/app.json" with { type: "json" };
import { loadWithEnv } from "../moduleTestSupport.ts";

const commandsModule = fileURLToPath(new URL("./commands.ts", import.meta.url));
type Loaded = { commands: Record<string, string>; downloads: Record<string, string>; productName: string };
const load = (env?: Record<string, string | undefined>) => loadWithEnv(commandsModule, ["commands", "downloads", "productName"], env) as unknown as Loaded;

test("by default the install command and the download addresses are the application settings' website's", () => {
  const { commands, downloads, productName } = load();
  const website = `${new URL(appConfig.websiteUrl).origin}/`;
  assert.equal(commands.install, `curl --proto '=https' --tlsv1.2 -fsSL ${website}install.sh | sh`);
  assert.deepEqual(downloads, {
    macos: `${website}download/macos/`,
    windows: `${website}download/windows/`,
    "windows-arm64": `${website}download/windows-arm64/`,
    "linux-x86_64": `${website}download/linux-x86_64/`,
    "linux-aarch64": `${website}download/linux-aarch64/`,
    releases: `${appConfig.repositoryUrl}/releases`,
  });
  assert.equal(productName, appConfig.name);
});

test("the install command and the download addresses follow the website's deployment origin and base path", () => {
  const { commands, downloads } = load({ NUXT_PUBLIC_SITE_ORIGIN: "https://example.com", NUXT_APP_BASE_URL: "product" });
  assert.equal(commands.install, "curl --proto '=https' --tlsv1.2 -fsSL https://example.com/product/install.sh | sh");
  assert.equal(downloads.macos, "https://example.com/product/download/macos/");
  assert.equal(downloads["linux-aarch64"], "https://example.com/product/download/linux-aarch64/");
  // The releases page is the repository's, not the website's.
  assert.equal(downloads.releases, `${appConfig.repositoryUrl}/releases`);
});

test("the default port is the one the application settings give", () => {
  const { commands } = load();
  assert.equal(commands.defaultPort, String(appConfig.defaultBrowserPort));
});

test("every listener the Tailscale and UU Remote guides start binds loopback alone, around the default port", () => {
  const { tunnel } = loadWithEnv(commandsModule, ["tunnel"]) as unknown as { tunnel: Record<string, string> };
  const port = appConfig.defaultBrowserPort;
  assert.match(tunnel.visitorForward, /^socat TCP-LISTEN:\d+,bind=127\.0\.0\.1,/);
  assert.ok(tunnel.visitorForward.startsWith(`socat TCP-LISTEN:${port},`));
  assert.ok(tunnel.visitorForwardWindows.includes(`listenaddress=127.0.0.1 listenport=${port} `));
  assert.equal(tunnel.serveBrowserPort, `tailscale serve --tcp=${port} tcp://127.0.0.1:${port}`);
  for (const command of Object.values(tunnel)) assert.equal(/funnel/i.test(command), false, command);
});

// The facts "Your system stays as it was" quotes are copies of what the daemon, the install script and the desktop
// application decide, which the docs read from none of them: each is held here to what decides it. What a Rust source
// decides is quoted in a fixture beside it, which that package's own test holds to the source; the rest is read as it
// stands. Each read names its file in full, so the verify scope's scan sees it (`scripts/verify/scope.mjs`).
const read = (url: URL) => readFileSync(url, "utf8");
const loadSystem = () => loadWithEnv(commandsModule, ["system"]).system as Record<string, string>;
const daemonQuoted = JSON.parse(read(new URL("../../../apps/daemon/testdata/docs_uninstall_command.json", import.meta.url))) as {
  cli_name: string;
  uninstall_options: string[];
  confirmation_word: string;
};
const desktopQuoted = JSON.parse(read(new URL("../../../apps/desktop/src-tauri/testdata/docs_system_paths.json", import.meta.url))) as {
  command_line_dirs: { system: string; user: string };
  clean_up_log_name: string;
};

test("the options of the daemon's uninstall command that the docs quote are the options it has", () => {
  const system = loadSystem();
  assert.deepEqual(
    [...daemonQuoted.uninstall_options].sort(),
    [system.uninstallYes, system.uninstallDryRun, system.uninstallKeepChanges, system.uninstallForce].sort(),
  );
  assert.equal(system.cliName, daemonQuoted.cli_name);
  assert.equal(system.uninstall, `${system.cliName} uninstall`);
  assert.equal(system.uninstallWord, daemonQuoted.confirmation_word);
});

test("the install script has the uninstall option and the install-folder option the docs quote", () => {
  const system = loadSystem();
  const script = read(new URL("../../../apps/web/public/install.sh", import.meta.url));
  assert.match(script, new RegExp(`^\\s*${system.binDirOption}\\)`, "m"));
  const option = /--uninstall$/.exec(system.uninstallScript);
  assert.ok(option, system.uninstallScript);
  assert.match(script, new RegExp(`^\\s*${option[0]}\\)`, "m"));
});

test("the folders and the log the docs name are the desktop application's: its identifier's folders, its links, the clean-up's own log", () => {
  const system = loadSystem();
  const { identifier } = JSON.parse(read(new URL("../../../apps/desktop/src-tauri/tauri.conf.json", import.meta.url))) as { identifier: string };
  assert.equal(system.appDataMac, `~/Library/Application Support/${identifier}`);
  assert.equal(system.appDataWindows, `%APPDATA%\\${identifier}`);
  assert.equal(system.appLocalDataWindows, `%LOCALAPPDATA%\\${identifier}`);
  assert.equal(system.cliDir, desktopQuoted.command_line_dirs.system, "the link's first place");
  assert.equal(system.cliFallbackDir, desktopQuoted.command_line_dirs.user, "the link's second place");
  // The log is the one the application writes, in the data directory's logs folder, and the one the installer's words
  // send the user to.
  const log = desktopQuoted.clean_up_log_name;
  assert.equal(system.uninstallLogWindows, `${system.dataDirWindows}\\logs\\${log}`);
  const hooks = read(new URL("../../../apps/desktop/src-tauri/windows/installer-hooks.nsh", import.meta.url));
  assert.ok(hooks.includes(`$PROFILE\\.octoboard\\logs\\${log}`));
});
