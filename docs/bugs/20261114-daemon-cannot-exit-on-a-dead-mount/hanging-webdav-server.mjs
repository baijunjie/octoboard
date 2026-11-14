// A minimal read-only WebDAV server: "/" holds the directories "ok" and "hang"; every request
// under "/hang/" (and for "/hang" itself) is accepted and never answered, like a server that has
// stopped responding. Logs each request.
import http from "node:http";
const port = Number(process.argv[2] || 47312);
const dirs = { "/": ["ok", "hang"], "/ok/": [], "/hang/": [] };
const date = new Date().toUTCString();
const entry = (href) => `<D:response><D:href>${href}</D:href><D:propstat><D:prop><D:resourcetype><D:collection/></D:resourcetype><D:getlastmodified>${date}</D:getlastmodified><D:creationdate>2026-01-01T00:00:00Z</D:creationdate><D:getcontentlength>0</D:getcontentlength><D:displayname>${href}</D:displayname></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response>`;
http.createServer((req, res) => {
  const path = decodeURIComponent(req.url.split("?")[0]);
  console.log(new Date().toISOString(), req.method, path, req.headers.depth ?? "");
  if (path === "/hang" || path.startsWith("/hang/")) return; // never answered
  if (req.method === "OPTIONS") {
    res.writeHead(200, { DAV: "1", Allow: "OPTIONS, GET, HEAD, PROPFIND", "Content-Length": 0 });
    return res.end();
  }
  if (req.method === "PROPFIND") {
    const key = path.endsWith("/") ? path : path + "/";
    if (!(key in dirs)) { res.writeHead(404, { "Content-Length": 0 }); return res.end(); }
    let body = `<?xml version="1.0" encoding="utf-8"?><D:multistatus xmlns:D="DAV:">${entry(key)}`;
    if (req.headers.depth !== "0") for (const d of dirs[key]) body += entry(`${key}${d}/`);
    body += "</D:multistatus>";
    res.writeHead(207, { "Content-Type": 'application/xml; charset="utf-8"', "Content-Length": Buffer.byteLength(body) });
    return res.end(body);
  }
  res.writeHead(req.method === "GET" || req.method === "HEAD" ? 404 : 405, { "Content-Length": 0 });
  res.end();
}).listen(port, "127.0.0.1", () => console.log("listening", port));
