import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import site from "../site.config.ts";

const root = fileURLToPath(new URL("../.output/public/", import.meta.url));
const port = Number(process.env.PORT || 4318);
const types = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".json": "application/json",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".webp": "image/webp",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
  ".xml": "application/xml",
  ".txt": "text/plain; charset=utf-8",
};

await stat(resolve(root, "index.html")).catch(() => {
  throw new Error(
    "Build the website before previewing it: pnpm --filter @octoboard/web build",
  );
});

const server = createServer(async (request, response) => {
  if (request.method !== "GET" && request.method !== "HEAD") {
    response.writeHead(405, { Allow: "GET, HEAD" }).end();
    return;
  }
  try {
    const url = new URL(request.url, "http://localhost");
    if (site.baseURL !== "/" && url.pathname === site.baseURL.slice(0, -1)) {
      response.writeHead(301, { Location: site.baseURL + url.search }).end();
      return;
    }
    if (url.pathname === "/" && site.baseURL !== "/") {
      response.writeHead(302, { Location: site.baseURL }).end();
      return;
    }
    if (!url.pathname.startsWith(site.baseURL))
      throw new Error("Outside site base");
    let path = resolve(
      root,
      decodeURIComponent(url.pathname.slice(site.baseURL.length)),
    );
    if (path !== resolve(root) && !path.startsWith(resolve(root) + sep))
      throw new Error("Outside public directory");
    if ((await stat(path)).isDirectory()) {
      if (!url.pathname.endsWith("/")) {
        response
          .writeHead(301, { Location: url.pathname + "/" + url.search })
          .end();
        return;
      }
      path = resolve(path, "index.html");
    }
    const data = await readFile(path);
    response.writeHead(200, {
      "Content-Type": types[extname(path)] || "application/octet-stream",
      "Content-Length": data.length,
      "Cache-Control": "no-store",
    });
    response.end(request.method === "HEAD" ? undefined : data);
  } catch {
    const page = await readFile(resolve(root, "404.html")).catch(() => null);
    response.writeHead(404, {
      "Content-Type": page
        ? "text/html; charset=utf-8"
        : "text/plain; charset=utf-8",
      "Cache-Control": "no-store",
    });
    response.end(request.method === "HEAD" ? undefined : page || "Not found");
  }
});
server.listen(port, "127.0.0.1", () => {
  console.log(`Website preview: http://127.0.0.1:${port}${site.baseURL}`);
});
