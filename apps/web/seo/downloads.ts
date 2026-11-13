// Where the site's download links point: the newest version, read from the update manifests when the site is
// generated. The manifests are the release's own record of what users should get (the stable ones take prereleases
// until a platform's first stable release), and GitHub's "latest release" cannot answer while every release is a
// prerelease. This module is pure apart from the injected `fetch`, so the tests run it without a network.

import appConfig from "../../../config/app.json" with { type: "json" };

// The downloads the site offers. `app` targets are desktop installers; `server` targets are the archives of the
// daemon (the Linux server form), one per architecture.
export const downloadTargets = [
  { id: "macos", name: "macOS", icon: "apple-logo", kind: "app" },
  { id: "windows", name: "Windows", icon: "windows-logo", kind: "app" },
  { id: "linux-x86_64", name: "Linux x86_64", icon: "linux-logo", kind: "server", architecture: "x86_64" },
  { id: "linux-aarch64", name: "Linux aarch64", icon: "linux-logo", kind: "server", architecture: "aarch64" },
] as const;

type DownloadTargetOf<Kind extends string> = Extract<(typeof downloadTargets)[number], { kind: Kind }>;

function targetsOfKind<Kind extends "app" | "server">(kind: Kind): DownloadTargetOf<Kind>[] {
  return downloadTargets.filter(
    (target): target is DownloadTargetOf<Kind> => target.kind === kind,
  );
}

export const appDownloads = targetsOfKind("app");
export const serverDownloads = targetsOfKind("server");

// The port `octoboardd start` listens on when none is given; config/app.json holds it for the daemon, the install
// script and this site alike.
export const defaultServerPort: number = appConfig.defaultServerPort;

// The commands the Linux copy quotes, keyed by the placeholder that names each in the catalogs. They are not
// translated.
export const linuxCommandParts = {
  start: "octoboardd start",
  binDir: "~/.local/bin",
  // The daemon serves the browser on an owner-only socket in its data directory, not on its port; `octoboardd
  // address` prints this command with the real path and user.
  forward: `ssh -o ExitOnForwardFailure=yes -L 127.0.0.1:${defaultServerPort}:/home/<you>/.octoboard/browser.sock <you>@<server>`,
  address: "octoboardd address",
  plain: "sh",
  option: "sh -s -- --service",
  port: "--port <port>",
  defaultPort: String(defaultServerPort),
  linger: 'loginctl enable-linger "$USER"',
  restart: "octoboardd restart",
} as const;

// The tabs of the download page, one per system the visitor can pick.
export const downloadTabs = [
  { id: "macos", name: "macOS", icon: "apple-logo" },
  { id: "windows", name: "Windows", icon: "windows-logo" },
  { id: "linux", name: "Linux", icon: "linux-logo" },
] as const;

// The catalog keys of the paragraphs under each desktop app's download button, in order.
export const appPanelKeys = {
  macos: ["downloadMacosRequirements", "downloadAgentsNote", "downloadUpdates"],
  windows: [
    "downloadWindowsRequirements",
    "downloadWindowsUserOnly",
    "downloadAgentsNote",
    "downloadWindowsGit",
    "downloadWindowsSmartScreen",
    "downloadUpdates",
  ],
} as const;

export type DownloadTab = (typeof downloadTabs)[number]["id"];

export function isDownloadTab(value: string): value is DownloadTab {
  return downloadTabs.some((tab) => tab.id === value);
}

// The tab that suits the visitor's system, from `navigator.userAgentData.platform` and `navigator.userAgent`.
// Phones and tablets have no app of their own here, so they get the first tab like an unknown system.
export function detectDownloadTab(platform: string, userAgent: string): DownloadTab {
  const value = `${platform} ${userAgent}`;
  if (/android|iphone|ipad|ipod|cros/i.test(value)) return "macos";
  if (/windows|win32|win64/i.test(value)) return "windows";
  if (/mac/i.test(value)) return "macos";
  if (/linux|x11/i.test(value)) return "linux";
  return "macos";
}

// The one-liner the site publishes. `--proto '=https' --tlsv1.2` keeps curl from following a redirect off https or
// settling for an old TLS version while it fetches the script, which runs before anything of it can check itself.
export function installCommand(siteUrl: string): string {
  return `curl --proto '=https' --tlsv1.2 -fsSL ${siteUrl}install.sh | sh`;
}

// Where a download's fixed redirect page lives, relative to the site root.
export function downloadPagePath(id: DownloadTarget): string {
  return `download/${id}/`;
}

// Whether the `CI` environment variable asks for strict resolution: set, and not an explicit "no".
export function strictFromEnv(ci: string | undefined): boolean {
  return ci !== undefined && !["", "false", "0"].includes(ci.toLowerCase());
}

export type DownloadTarget = (typeof downloadTargets)[number]["id"];
export type Downloads = Record<DownloadTarget, string>;

type FetchLike = (
  url: string,
  init?: { signal?: AbortSignal },
) => Promise<{
  status: number;
  ok: boolean;
  json(): Promise<unknown>;
}>;

const MANIFEST_RELEASE_TAG = "updater";
const SEMVER =
  /^\d+\.\d+\.\d+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
const FETCH_TIMEOUT_MS = 15_000;

function trimmed(repositoryUrl: string): string {
  return repositoryUrl.replace(/\/$/, "");
}

export function releasesUrl(repositoryUrl: string): string {
  return `${trimmed(repositoryUrl)}/releases`;
}

function releaseDownloadPrefix(repositoryUrl: string): string {
  return `${releasesUrl(repositoryUrl)}/download/`;
}

export function manifestUrl(repositoryUrl: string, name: string): string {
  return `${releaseDownloadPrefix(repositoryUrl)}${MANIFEST_RELEASE_TAG}/${name}`;
}

// Every target pointing at the releases page: what a platform without a manifest links to.
export function fallbackDownloads(repositoryUrl: string): Downloads {
  const url = releasesUrl(repositoryUrl);
  return {
    macos: url,
    windows: url,
    "linux-x86_64": url,
    "linux-aarch64": url,
  };
}

function record(value: unknown, what: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${what} is not an object`);
  }
  return value as Record<string, unknown>;
}

function manifestVersion(manifest: unknown, what: string): string {
  const version = record(manifest, what).version;
  if (typeof version !== "string" || !SEMVER.test(version)) {
    throw new Error(`${what} has no valid version`);
  }
  return version;
}

// A download must stay on the repository's own release assets.
function releaseAsset(repositoryUrl: string, value: unknown, what: string): string {
  if (typeof value !== "string") throw new Error(`${what} has no url`);
  let href: string;
  try {
    const url = new URL(value);
    if (url.protocol !== "https:") throw new Error("not https");
    href = url.href;
  } catch {
    throw new Error(`${what} has an invalid url`);
  }
  if (!href.startsWith(releaseDownloadPrefix(repositoryUrl))) {
    throw new Error(`${what} is not a release download of the repository`);
  }
  return href;
}

function platformUrl(
  repositoryUrl: string,
  manifest: unknown,
  key: string,
  what: string,
): string {
  const platforms = record(record(manifest, what).platforms, `${what} platforms`);
  const entry = record(platforms[key], `${what} platform ${key}`);
  return releaseAsset(repositoryUrl, entry.url, `${what} platform ${key}`);
}

// The macOS manifest's url is the update archive; the download is the disk image of the same version.
export function macosDownload(
  repositoryUrl: string,
  productName: string,
  manifest: unknown,
): string {
  const version = manifestVersion(manifest, "stable.json");
  const platforms = record(record(manifest, "stable.json").platforms, "stable.json platforms");
  if (!Object.keys(platforms).some((key) => key.startsWith("darwin-"))) {
    throw new Error("stable.json has no macOS platform");
  }
  return releaseAsset(
    repositoryUrl,
    `${releaseDownloadPrefix(repositoryUrl)}desktop-v${version}/${productName}_${version}_universal.dmg`,
    "stable.json disk image",
  );
}

export function windowsDownload(repositoryUrl: string, manifest: unknown): string {
  manifestVersion(manifest, "stable-windows.json");
  return platformUrl(repositoryUrl, manifest, "windows-x86_64", "stable-windows.json");
}

export function linuxDownloads(
  repositoryUrl: string,
  manifest: unknown,
): Pick<Downloads, "linux-x86_64" | "linux-aarch64"> {
  manifestVersion(manifest, "stable-linux.json");
  return {
    "linux-x86_64": platformUrl(repositoryUrl, manifest, "linux-x86_64", "stable-linux.json"),
    "linux-aarch64": platformUrl(repositoryUrl, manifest, "linux-aarch64", "stable-linux.json"),
  };
}

// The parsed manifest, or null when the release holds none (404). Any other failure throws.
async function readManifest(fetchManifest: FetchLike, url: string): Promise<unknown> {
  const response = await fetchManifest(url, {
    signal: AbortSignal.timeout(FETCH_TIMEOUT_MS),
  });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`${url} answered ${response.status}`);
  return response.json();
}

export type ResolveOptions = {
  repositoryUrl: string;
  productName: string;
  fetch: FetchLike;
  // Fail instead of falling back when a manifest cannot be read (CI, where a stale deploy must not go out unseen).
  strict: boolean;
  warn: (message: string) => void;
};

export async function resolveDownloads(options: ResolveOptions): Promise<Downloads> {
  const { repositoryUrl, productName, strict, warn } = options;
  const downloads = fallbackDownloads(repositoryUrl);
  const groups: Array<[string, (manifest: unknown) => Partial<Downloads>]> = [
    ["stable.json", (manifest) => ({ macos: macosDownload(repositoryUrl, productName, manifest) })],
    ["stable-windows.json", (manifest) => ({ windows: windowsDownload(repositoryUrl, manifest) })],
    ["stable-linux.json", (manifest) => linuxDownloads(repositoryUrl, manifest)],
  ];
  await Promise.all(
    groups.map(async ([name, derive]) => {
      try {
        const manifest = await readManifest(options.fetch, manifestUrl(repositoryUrl, name));
        if (manifest === null) {
          warn(`${name} does not exist yet; its download links point at the releases page`);
        } else {
          Object.assign(downloads, derive(manifest));
        }
      } catch (error) {
        const reason = error instanceof Error ? error.message : String(error);
        if (strict) throw new Error(`Cannot resolve the downloads from ${name}: ${reason}`, { cause: error });
        warn(`Download links fall back to the releases page for ${name}: ${reason}`);
      }
    }),
  );
  return downloads;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

// The page at a fixed download address: an immediate redirect with a plain link behind it. It is unlocalized,
// not indexed and not in the sitemap, like the 404 page, so its few sentences are English only.
function downloadRedirectPage(input: {
  url: string;
  title: string;
  baseURL: string;
}): string {
  const url = escapeHtml(input.url);
  const script = JSON.stringify(input.url).replaceAll("<", "\\u003c");
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(input.title)}</title>
<meta name="robots" content="noindex, nofollow">
<meta http-equiv="refresh" content="0; url=${url}">
<link rel="icon" type="image/png" href="${escapeHtml(input.baseURL)}favicon.png">
<style>
body { margin: 0; min-height: 100vh; display: grid; place-content: center; gap: 16px; padding: 24px; background: #000; color: #f5f5f5; font: 16px/1.6 system-ui, sans-serif; text-align: center; }
a { color: #f5f5f5; }
</style>
<script>location.replace(${script});</script>
</head>
<body>
<p>Taking you to the download. If nothing happens, use the link below.</p>
<p><a href="${url}">${escapeHtml(input.title)}</a></p>
</body>
</html>
`;
}

// Every redirect page of the site, keyed by its path in the generated output.
export function renderDownloadPages(input: {
  downloads: Downloads;
  baseURL: string;
}): Record<string, string> {
  return Object.fromEntries(
    downloadTargets.map(({ id, name }) => [
      `${downloadPagePath(id)}index.html`,
      downloadRedirectPage({
        url: input.downloads[id],
        title: `Download for ${name}`,
        baseURL: input.baseURL,
      }),
    ]),
  );
}
