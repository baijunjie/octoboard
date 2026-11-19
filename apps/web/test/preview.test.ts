import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { createServer } from "node:net";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

// `scripts/preview.mjs` serves the built website, and the docs from their own build under `<base>docs/`. It resolves its
// roots from its own location, so each test runs a copy of it (and of the site settings it imports) in a throwaway
// tree of the same shape, with pages of its own, and no build is needed.

const webDirectory = fileURLToPath(new URL("../", import.meta.url));
const appConfig = fileURLToPath(new URL("../../../config/app.json", import.meta.url));

type Preview = { port: number; stderr: () => string; get: (target: string, method?: string) => Promise<Reply>; stop: () => Promise<void> };
type Reply = { status: number; headers: Record<string, string | string[] | undefined>; body: string };

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address() as { port: number };
      probe.close(() => resolve(port));
    });
  });
}

function write(file: string, content: string) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, content);
}

/** The tree: the website's output with a home page, a 404 page and a secret next to the docs' build, and, when `docs`
 * is set, the docs' build with its pages, a script and a WebAssembly file. */
function buildTree(docs: boolean): string {
  const tree = mkdtempSync(path.join(os.tmpdir(), "octoboard-preview-"));
  cpSync(path.join(webDirectory, "scripts/preview.mjs"), path.join(tree, "apps/web/scripts/preview.mjs"));
  cpSync(path.join(webDirectory, "site.config.ts"), path.join(tree, "apps/web/site.config.ts"));
  cpSync(appConfig, path.join(tree, "config/app.json"));
  const output = path.join(tree, "apps/web/.output/public");
  write(path.join(output, "index.html"), "website home");
  write(path.join(output, "download/index.html"), "website download");
  write(path.join(output, "404.html"), "website not found");
  write(path.join(tree, "apps/docs/secret.txt"), "docs secret");
  write(path.join(tree, "apps/web/.output/secret.txt"), "output secret");
  if (docs) {
    const dist = path.join(tree, "apps/docs/dist");
    write(path.join(dist, "index.html"), "docs home");
    write(path.join(dist, "install/linux/index.html"), "docs linux");
    write(path.join(dist, "zh-CN/index.html"), "docs chinese");
    write(path.join(dist, "pagefind/pagefind.wasm"), "\0asm");
    write(path.join(dist, "pagefind/pagefind.js"), "export {}");
  }
  return tree;
}

async function startPreview(tree: string, env: Record<string, string> = {}): Promise<Preview> {
  const port = await freePort();
  const child: ChildProcess = spawn(
    process.execPath,
    ["--experimental-strip-types", "--no-warnings", path.join(tree, "apps/web/scripts/preview.mjs")],
    {
      env: { ...process.env, NUXT_APP_BASE_URL: "", NUXT_PUBLIC_SITE_ORIGIN: "", ...env, PORT: String(port) },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  let stderr = "";
  child.stderr!.setEncoding("utf8").on("data", (chunk) => (stderr += chunk));
  const exited = new Promise<void>((resolve) => child.once("exit", () => resolve()));
  await new Promise<void>((resolve, reject) => {
    let stdout = "";
    child.stdout!.setEncoding("utf8").on("data", (chunk) => {
      stdout += chunk;
      if (stdout.includes("Website preview:")) resolve();
    });
    void exited.then(() => reject(new Error(`the preview exited before it listened:\n${stderr}`)));
  });
  return {
    port,
    stderr: () => stderr,
    // `fetch` would normalise dot segments out of the path; this sends it as written.
    get: (target, method = "GET") =>
      new Promise((resolve, reject) => {
        const outgoing = request({ host: "127.0.0.1", port, path: target, method }, (reply) => {
          let body = "";
          reply.setEncoding("utf8").on("data", (chunk) => (body += chunk));
          reply.on("end", () => resolve({ status: reply.statusCode!, headers: reply.headers, body }));
        });
        outgoing.on("error", reject);
        outgoing.end();
      }),
    stop: async () => {
      child.kill();
      await exited;
    },
  };
}

async function withPreview(options: { docs: boolean; env?: Record<string, string> }, run: (preview: Preview) => Promise<void>) {
  const tree = buildTree(options.docs);
  const preview = await startPreview(tree, options.env);
  try {
    await run(preview);
  } finally {
    await preview.stop();
    rmSync(tree, { recursive: true, force: true });
  }
}

test("the preview serves the docs' build under docs/ and the website's output everywhere else", async () => {
  await withPreview({ docs: true }, async (preview) => {
    assert.equal(preview.stderr(), "");
    assert.equal((await preview.get("/")).body, "website home");
    assert.equal((await preview.get("/download/")).body, "website download");
    const home = await preview.get("/docs/");
    assert.equal(home.status, 200);
    assert.equal(home.body, "docs home");
    assert.match(String(home.headers["content-type"]), /^text\/html/);
    assert.equal((await preview.get("/docs/install/linux/")).body, "docs linux");
    assert.equal((await preview.get("/docs/zh-CN/")).body, "docs chinese");
    assert.equal((await preview.get("/docs/install/linux/", "HEAD")).body, "");
  });
});

test("the docs' address without its trailing slash is sent to the one with it, keeping the query", async () => {
  await withPreview({ docs: true }, async (preview) => {
    for (const [from, to] of [["/docs", "/docs/"], ["/docs?q=1", "/docs/?q=1"], ["/docs/install/linux", "/docs/install/linux/"]]) {
      const reply = await preview.get(from);
      assert.equal(reply.status, 301, from);
      assert.equal(reply.headers.location, to, from);
    }
  });
});

test("the docs' WebAssembly file is served as WebAssembly, and a script as a script", async () => {
  await withPreview({ docs: true }, async (preview) => {
    const wasm = await preview.get("/docs/pagefind/pagefind.wasm");
    assert.equal(wasm.status, 200);
    assert.equal(wasm.headers["content-type"], "application/wasm");
    assert.match(String((await preview.get("/docs/pagefind/pagefind.js")).headers["content-type"]), /^text\/javascript/);
  });
});

test("a path out of the docs' build is refused, as is one the build has no page for, with the website's 404 page", async () => {
  await withPreview({ docs: true }, async (preview) => {
    for (const target of ["/docs/%2e%2e/secret.txt", "/docs/../secret.txt", "/docs/..%2f..%2f.output/secret.txt", "/docs/missing/"]) {
      const reply = await preview.get(target);
      assert.equal(reply.status, 404, target);
      assert.equal(reply.body, "website not found", target);
    }
    // A path out of the website's output is refused as before.
    const reply = await preview.get("/..%2fsecret.txt");
    assert.equal(reply.status, 404);
    assert.equal(reply.body.includes("secret"), false);
  });
});

test("without a docs build the preview warns, names the command, and still serves the website", async () => {
  await withPreview({ docs: false }, async (preview) => {
    assert.equal((await preview.get("/")).body, "website home");
    const missing = await preview.get("/docs/");
    assert.equal(missing.status, 404);
    assert.equal(missing.body, "website not found");
    // The warning is written before the server listens, so it is in the pipe once the server answers.
    assert.match(preview.stderr(), /The docs are not built/);
    assert.match(preview.stderr(), /pnpm --filter @octoboard\/docs build/);
  });
});

test("under a base path the docs are at docs/ below it", async () => {
  await withPreview({ docs: true, env: { NUXT_APP_BASE_URL: "/site/" } }, async (preview) => {
    assert.equal((await preview.get("/site/")).body, "website home");
    assert.equal((await preview.get("/site/docs/")).body, "docs home");
    assert.equal((await preview.get("/site/docs/install/linux/")).body, "docs linux");
    const reply = await preview.get("/site/docs");
    assert.equal(reply.status, 301);
    assert.equal(reply.headers.location, "/site/docs/");
    // The docs' own segment is not served at the root of a site that has a base path.
    assert.equal((await preview.get("/docs/")).status, 404);
  });
});
