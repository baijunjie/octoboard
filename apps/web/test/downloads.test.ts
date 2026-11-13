import assert from "node:assert/strict";
import test from "node:test";
import { existsSync, readFileSync } from "node:fs";
import {
  detectDownloadTab,
  downloadPagePath,
  downloadTargets,
  fallbackDownloads,
  installCommand,
  manifestUrl,
  renderDownloadPages,
  resolveDownloads,
  strictFromEnv,
} from "../seo/downloads.ts";

const repositoryUrl = "https://github.com/baijunjie/octoboard";
const base = `${repositoryUrl}/releases/download`;
const releases = `${repositoryUrl}/releases`;

const macos = { version: "0.1.0-beta.5", platforms: { "darwin-aarch64": { url: `${base}/desktop-v0.1.0-beta.5/Octoboard_0.1.0-beta.5_universal.app.tar.gz` } } };
const windows = {
  version: "0.1.0-beta.5",
  platforms: { "windows-x86_64": { url: `${base}/desktop-v0.1.0-beta.5/Octoboard_0.1.0-beta.5_x64-setup.exe` } },
};
const linux = {
  version: "0.1.0",
  platforms: {
    "linux-x86_64": { url: `${base}/desktop-v0.1.0/octoboardd_0.1.0_linux_x86_64.tar.gz` },
    "linux-aarch64": { url: `${base}/desktop-v0.1.0/octoboardd_0.1.0_linux_aarch64.tar.gz` },
  },
};

type Answer = { status: number; body?: unknown };

function fetcher(answers: Record<string, Answer | Error>) {
  return async (url: string) => {
    const answer = answers[url.slice(manifestUrl(repositoryUrl, "").length)];
    if (answer === undefined) throw new Error(`unexpected ${url}`);
    if (answer instanceof Error) throw answer;
    return {
      status: answer.status,
      ok: answer.status >= 200 && answer.status < 300,
      json: async () => {
        if (answer.body === undefined) throw new SyntaxError("Unexpected token");
        return answer.body;
      },
    };
  };
}

function options(answers: Record<string, Answer | Error>, strict = false) {
  const warnings: string[] = [];
  return {
    warnings,
    options: {
      repositoryUrl,
      productName: "Octoboard",
      fetch: fetcher(answers),
      strict,
      warn: (message: string) => warnings.push(message),
    },
  };
}

test("each platform takes its link from its manifest", async () => {
  const { options: run, warnings } = options({
    "stable.json": { status: 200, body: macos },
    "stable-windows.json": { status: 200, body: windows },
    "stable-linux.json": { status: 200, body: linux },
  });
  assert.deepEqual(await resolveDownloads(run), {
    macos: `${base}/desktop-v0.1.0-beta.5/Octoboard_0.1.0-beta.5_universal.dmg`,
    windows: `${base}/desktop-v0.1.0-beta.5/Octoboard_0.1.0-beta.5_x64-setup.exe`,
    "linux-x86_64": `${base}/desktop-v0.1.0/octoboardd_0.1.0_linux_x86_64.tar.gz`,
    "linux-aarch64": `${base}/desktop-v0.1.0/octoboardd_0.1.0_linux_aarch64.tar.gz`,
  });
  assert.deepEqual(warnings, []);
});

test("a missing manifest falls back to the releases page for that platform, even in strict mode", async () => {
  const { options: run, warnings } = options(
    {
      "stable.json": { status: 200, body: macos },
      "stable-windows.json": { status: 200, body: windows },
      "stable-linux.json": { status: 404 },
    },
    true,
  );
  const downloads = await resolveDownloads(run);
  assert.equal(downloads["linux-x86_64"], releases);
  assert.equal(downloads["linux-aarch64"], releases);
  assert.match(downloads.macos, /universal\.dmg$/);
  assert.equal(warnings.length, 1);
  assert.match(warnings[0], /stable-linux\.json does not exist/);
});

test("other failures warn and fall back, or fail when strict", async () => {
  const answers = {
    "stable.json": { status: 500 },
    "stable-windows.json": new Error("network down"),
    "stable-linux.json": { status: 200 },
  };
  const loose = options(answers);
  assert.deepEqual(await resolveDownloads(loose.options), fallbackDownloads(repositoryUrl));
  assert.equal(loose.warnings.length, 3);
  await assert.rejects(resolveDownloads(options(answers, true).options), /Cannot resolve the downloads from/);
});

test("a manifest with a bad version or a url outside the repository's releases is rejected", async () => {
  const badVersion = options({
    "stable.json": { status: 200, body: { ...macos, version: "../../evil" } },
    "stable-windows.json": { status: 200, body: windows },
    "stable-linux.json": { status: 404 },
  }, true);
  await assert.rejects(resolveDownloads(badVersion.options), /no valid version/);
  for (const url of ["https://example.com/setup.exe", `${base}/../../evil.exe`, "http://github.com/baijunjie/octoboard/releases/download/x/y.exe"]) {
    const bad = options({
      "stable.json": { status: 200, body: macos },
      "stable-windows.json": { status: 200, body: { ...windows, platforms: { "windows-x86_64": { url } } } },
      "stable-linux.json": { status: 404 },
    }, true);
    await assert.rejects(resolveDownloads(bad.options), /stable-windows\.json/, url);
  }
});

test("a manifest lacking a Linux architecture is rejected as a whole", async () => {
  const half = { version: "0.1.0", platforms: { "linux-x86_64": linux.platforms["linux-x86_64"] } };
  const { options: run, warnings } = options({
    "stable.json": { status: 404 },
    "stable-windows.json": { status: 404 },
    "stable-linux.json": { status: 200, body: half },
  });
  assert.deepEqual(await resolveDownloads(run), fallbackDownloads(repositoryUrl));
  assert.equal(warnings.filter((warning) => warning.includes("stable-linux.json: ")).length, 1);
});

test("a macOS manifest without a macOS platform is rejected", async () => {
  const { options: run } = options(
    {
      "stable.json": { status: 200, body: { version: "0.1.0", platforms: { "windows-x86_64": {} } } },
      "stable-windows.json": { status: 404 },
      "stable-linux.json": { status: 404 },
    },
    true,
  );
  await assert.rejects(resolveDownloads(run), /no macOS platform/);
});

test("CI is strict unless unset or an explicit no", () => {
  assert.equal(strictFromEnv(undefined), false);
  for (const value of ["", "false", "FALSE", "0"]) assert.equal(strictFromEnv(value), false, value);
  for (const value of ["true", "1", "yes"]) assert.equal(strictFromEnv(value), true, value);
});

test("the fallback downloads all point at the releases page", () => {
  assert.deepEqual(new Set(Object.values(fallbackDownloads(`${repositoryUrl}/`))), new Set([releases]));
});

test("a redirect page exists for every target and names its resolved download", () => {
  const downloads = fallbackDownloads(repositoryUrl);
  downloads.windows = `${base}/desktop-v1.0.0/Octoboard_1.0.0_x64-setup.exe`;
  const pages = renderDownloadPages({ downloads, baseURL: "/site/" });
  assert.deepEqual(
    Object.keys(pages).sort(),
    downloadTargets.map(({ id }) => `${downloadPagePath(id)}index.html`).sort(),
  );
  const windows = pages["download/windows/index.html"];
  assert.match(windows, /<title>Download for Windows<\/title>/);
  assert.match(windows, /<meta name="robots" content="noindex, nofollow">/);
  assert.match(windows, /<meta http-equiv="refresh" content="0; url=[^"]*x64-setup\.exe">/);
  assert.match(windows, /location\.replace\("[^"]*x64-setup\.exe"\)/);
  assert.match(windows, /href="\/site\/favicon\.png"/);
  assert.match(pages["download/macos/index.html"], new RegExp(`url=${releases}"`));
});

test("the install command points at the site's script", () => {
  assert.equal(installCommand("https://octoboard.dev/"), "curl -fsSL https://octoboard.dev/install.sh | sh");
  assert.equal(installCommand("https://example.com/product/"), "curl -fsSL https://example.com/product/install.sh | sh");
});

test("the redirect page escapes the target and carries a plain link", () => {
  const downloads = { ...fallbackDownloads(repositoryUrl), windows: `${base}/a"b</script>.exe` };
  const page = renderDownloadPages({ downloads, baseURL: "/" })["download/windows/index.html"];
  assert.match(page, /noindex/);
  assert.match(page, /<meta http-equiv="refresh" content="0; url=[^"]*&quot;b/);
  assert.equal(page.includes("</script>.exe"), false);
  assert.match(page, /<a href="[^"]*">Download for Windows<\/a>/);
});

// The generated output of a default-environment build (production origin, root base path).
const generatedOutput = new URL("../.output/public/", import.meta.url);
const generatedSitemap = new URL("sitemap.xml", generatedOutput);

test(
  "the generated site serves the install script and redirects at fixed download addresses",
  {
    skip: !existsSync(generatedSitemap)
      ? "run pnpm --filter @octoboard/web build first"
      : false,
  },
  () => {
    const script = readFileSync(new URL("install.sh", generatedOutput), "utf8");
    assert.equal(script, readFileSync(new URL("../public/install.sh", import.meta.url), "utf8"));
    for (const target of ["macos", "windows", "linux-x86_64", "linux-aarch64"]) {
      const page = readFileSync(new URL(`download/${target}/index.html`, generatedOutput), "utf8");
      assert.match(page, /<meta name="robots" content="noindex, nofollow">/);
      assert.match(page, /<meta http-equiv="refresh" content="0; url=https:\/\/github\.com\/baijunjie\/octoboard\/releases/);
      assert.match(page, /location\.replace\("https:\/\/github\.com\/baijunjie\/octoboard\/releases/);
      assert.match(page, /<a href="https:\/\/github\.com\/baijunjie\/octoboard\/releases/);
    }
    const page = readFileSync(new URL("download/index.html", generatedOutput), "utf8");
    assert.ok(page.includes("curl -fsSL https://octoboard.dev/install.sh | sh"));
    assert.ok(page.includes("x86_64 archive"));
    assert.ok(page.includes('<meta name="robots"') === false);
    assert.ok(readFileSync(new URL("download.md", generatedOutput), "utf8").includes("# Download Octoboard"));
  },
);

test("the download tab follows the visitor's system and falls back to macOS", () => {
  assert.equal(detectDownloadTab("Windows", ""), "windows");
  assert.equal(detectDownloadTab("", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"), "windows");
  assert.equal(detectDownloadTab("macOS", ""), "macos");
  assert.equal(detectDownloadTab("", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)"), "macos");
  assert.equal(detectDownloadTab("Linux", ""), "linux");
  assert.equal(detectDownloadTab("", "Mozilla/5.0 (X11; Ubuntu; Linux x86_64)"), "linux");
  assert.equal(detectDownloadTab("", "Mozilla/5.0 (Linux; Android 14)"), "macos");
  assert.equal(detectDownloadTab("", "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)"), "macos");
  assert.equal(detectDownloadTab("", ""), "macos");
});
