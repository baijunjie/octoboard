import assert from "node:assert/strict";
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
