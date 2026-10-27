// @vitest-environment jsdom
import { expect, it } from "vitest";

import { appConfig } from "./appConfig";
import { DaemonRequestError } from "./daemon-client";
import { ActionableError } from "./actionableError";
import { issueReportUrl, osFromUserAgent, recordNames, sanitizeReportText, type IssueFacts } from "./issueReport";

const context = { names: ["Secret Project", "Q", "zz", "my-branch", "Q7", "dev"], homeDir: "/Users/alice" };
const sanitize = (text: string) => sanitizeReportText(text, context);

it.each([
  ["record names, longest first, and not one too short to be a name", "opened Secret Project on zz and Q", "opened <name> on <name> and Q"],
  ["the known home directory", "ENOENT /Users/alice/code/app.ts", "ENOENT <path>"],
  ["any other user's home directory", "at /home/bob and C:\\Users\\Bob\\x\\y.txt", "at ~ and <path>"],
  ["other POSIX paths", "failed to read /var/data/db.sqlite: denied", "failed to read <path>: denied"],
  ["Windows paths", "failed to read D:\\work\\db.sqlite now", "failed to read <path> now"],
  ["a file URL", "from file:///opt/app/main.js", "from <path>"],
  ["a URL that is not the application's own, whole", "GET https://github.com/acme/private-repo?token=1#top failed", "GET <url> failed"],
  ["a URL with userinfo, whole, the password with it", "https://alice:hunter2@intranet/x", "<url>"],
  ["an scp-style git remote", "clone git@github.com:acme/private-repo.git failed", "clone <url> failed"],
  ["the query and fragment of the application's own URL", "at http://127.0.0.1:5173/src/a.tsx?t=1#x:3:4", "at http://127.0.0.1:5173/src/a.tsx:3:4"],
  ["an email address with a record name in it", "mail alice@acme-corp.com", "mail <email>"],
  ["a token with a record name in it", "Bearer xK9mQ7pLdevR2tY8wZ3nB6vC1 and 9f8e7d6c5b4a3b2c1d0e9f8a7b6dq7", "<redacted> and <redacted>"],
  ["a secret assigned to a key with a prefix or suffix", "DB_PASSWORD=hunter2 GITHUB_TOKEN=abc AWS_SECRET_ACCESS_KEY=xyz client_secret: s3 OPENAI_API_KEY=sk-proj-abc", "DB_PASSWORD=<redacted> GITHUB_TOKEN=<redacted> AWS_SECRET_ACCESS_KEY=<redacted> client_secret: <redacted> OPENAI_API_KEY=<redacted>"],
  ["a cookie line", "Cookie: sid=abc; theme=dark", "Cookie: <redacted>"],
  ["a Windows home with forward slashes, whole", "C:/Users/Bob/x", "<path>"],
  ["a record name only where it stands alone", "Qx devil dev", "Qx devil <name>"],
  ["email addresses", "sent to jane.doe+x@example.co.uk", "sent to <email>"],
  ["IP addresses but not loopback", "connect 192.168.1.20 then 127.0.0.1 and fe80::1ff:fe23:4567:890a vs ::1", "connect <ip> then 127.0.0.1 and <ip> vs ::1"],
  ["UUIDs", "session 123e4567-e89b-12d3-a456-426614174000 gone", "session <id> gone"],
  ["secret-looking tokens", "key sk-abcdefghijklmnop1234 and ghp_abcdefghijklmnopqrstuv and Bearer abc.def-123456 and a1b2c3d4e5f6a7b8c9d0e1", "key <redacted> and <redacted> and <redacted> and <redacted>"],
  ["a secret assigned to a named key", "password: \"hunter2\" api_key=abc", "password: <redacted> api_key=<redacted>"],
  ["bidirectional isolates", "\u2068name\u2069 \u2066x\u2067", "name x"],
])("sanitizes %s", (_name, input, expected) => {
  expect(sanitize(input)).toBe(expected);
});

it.each([
  ["with the home directory known", "/Users/alice"],
  ["without it", null],
])("takes the whole path of a project whose directory has spaces, %s", (_name, homeDir) => {
  const withPaths = { names: ["/Users/alice/My Clients/Acme Corp", "/Volumes/Work Disk/Acme Corp"], homeDir };
  expect(sanitizeReportText("open /Users/alice/My Clients/Acme Corp/notes.md failed", withPaths)).toBe("open <path> failed");
  // Its `~` form only where the home directory is known to expand it.
  if (homeDir) expect(sanitizeReportText("open ~/My Clients/Acme Corp/notes.md failed", withPaths)).toBe("open <path> failed");
  expect(sanitizeReportText("open /Volumes/Work Disk/Acme Corp/notes.md failed", withPaths)).toBe("open <path> failed");
});

it("does not take a sibling directory that starts like a project's for the project", () => {
  const siblings = { names: ["/Users/alice/proj"], homeDir: "/Users/alice" };
  expect(sanitizeReportText("/Users/alice/proj-old/secret-plan.md and /Users/alice/proj.bak/x and /Users/alice/proj/y", siblings)).toBe("<path> and <path> and <path>");
  expect(sanitizeReportText("in /Users/alice/proj-old", { names: ["/Users/alice/proj"] })).toBe("in <path>");
  expect(sanitizeReportText("in /Users/alice/proj-old", { names: ["/work/proj"] })).toBe("in <path>");
  expect(sanitizeReportText("/work/proj-old/x", { names: ["/work/proj"] })).toBe("<path>");
  expect(sanitizeReportText("/work/proj.", { names: ["/work/proj"] })).toBe("<path>.");
  for (const sibling of ["proj+old", "proj~1", "proj@2", "proj(1)", "proj_x"]) {
    // Redacted as a path of its own, not as the project's with the rest of the sibling's name left on it.
    expect(sanitizeReportText(`/work/${sibling}/x`, { names: ["/work/proj"] })).not.toContain(sibling.slice(4));
  }
  expect(sanitizeReportText("open '/work/proj'; /work/proj, /work/proj)", { names: ["/work/proj"] })).toBe("open '<path>'; <path>, <path>)");
});

it("replaces a URL that holds a project path whole", () => {
  expect(sanitizeReportText("https://evil.com/cb?dir=/Users/alice/proj&code=OAUTH123&state=xyz", { names: ["/Users/alice/proj"], homeDir: "/Users/alice" })).toBe("<url>");
});

it("leaves the home directory out of an application URL's path and keeps the rest of its frames", () => {
  const stack = ["at http://localhost:5173/@fs/Users/bob/octo/src/a.tsx:1:2", "at http://localhost:5173/@fs/C:/Users/bob/octo/b.ts?t=1:3:4", "at tauri://localhost/assets/index-ab12.js:5:6", "at http://localhost:5173/src/c.tsx:7:8"].join("\n");
  expect(sanitizeReportText(stack, { names: [] })).toBe(["at http://localhost:5173/@fs/~/octo/src/a.tsx:1:2", "at http://localhost:5173/@fs/~/octo/b.ts:3:4", "at tauri://localhost/assets/index-ab12.js:5:6", "at http://localhost:5173/src/c.tsx:7:8"].join("\n"));
  expect(sanitizeReportText("http://localhost:5173/@fs/data/carol/x.ts:1:1", { names: [], homeDir: "/data/carol" })).toBe("http://localhost:5173/@fs/~/x.ts:1:1");
});

it("sanitizes pathological input quickly, cutting a long text before it", () => {
  const started = performance.now();
  for (const input of ["a".repeat(100_000), "a@".repeat(50_000), "password".repeat(12_500), "1".repeat(100_000), "/a".repeat(50_000)]) {
    const sanitized = sanitize(input);
    expect(sanitized.length).toBeLessThan(20_000);
    expect(sanitized.endsWith("… (truncated)")).toBe(true);
  }
  expect(performance.now() - started).toBeLessThan(2000);
});

it("treats a failure its caller calls unexpected as one, whatever its text alone says", () => {
  const error = new Error("render failed (os error 28)");
  const { title, body } = parse(issueReportUrl(BASE, facts([{ error, unexpected: true }])));
  expect(title).toBe("Unexpected error: render failed (os error 28)");
  expect(body).toContain("- Kind: unexpected");
  expect(parse(issueReportUrl(BASE, facts([{ error }]))).body).toContain("- Kind: actionable");
});

it("replaces a home directory only up to a segment boundary", () => {
  const home = { names: [], homeDir: "/data/alice" };
  expect(sanitizeReportText("/data/alicex/foo vs /data/alice", home)).toBe("<path> vs ~");
  expect(sanitizeReportText("in /data/alice: failed", home)).toBe("in ~: failed");
});

it("keeps the addresses of the application's own scripts in a stack, with their positions", () => {
  const stack = [
    "Error: boom",
    "    at render (tauri://localhost/assets/index-Ab12Cd34.js:1:2345)",
    "    at http://localhost:5173/src/App.tsx?t=17:10:2",
    "    at /Users/alice/code/octoboard/src/App.tsx:10:2",
  ].join("\n");
  expect(sanitize(stack)).toBe([
    "Error: boom",
    "    at render (tauri://localhost/assets/index-Ab12Cd34.js:1:2345)",
    "    at http://localhost:5173/src/App.tsx:10:2",
    "    at <path>:10:2",
  ].join("\n"));
});

it("leaves identifiers and plain words of the code alone", () => {
  const text = "TypeError: renderWithHooksAndSomethingVeryLong is not a function (std::fmt::Error, and/or 12:30:45)";
  expect(sanitize(text)).toBe(text);
});

it("takes the operating system from a user agent, coarsely", () => {
  expect(osFromUserAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe("macOS 10.15.7");
  expect(osFromUserAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120")).toBe("Windows NT 10.0");
  expect(osFromUserAgent("Mozilla/5.0 (X11; Linux x86_64)")).toBe("Linux");
  expect(osFromUserAgent(undefined)).toBeUndefined();
});

it("collects the names of the user's records, leaving out those too short", () => {
  const state = {
    consoles: new Map([["c", { name: "Main console" }]]),
    projects: new Map([["p", { name: "Octo", path: "/work/octo", remote_url: "git@host:me/octo.git" }]]),
    sessions: new Map([["s", { title: "Fix login", terminal_title: "zsh - fix" }]]),
    shellTerminals: new Map([["t", { title: "x" }]]),
    gitStatuses: new Map([["p", { branch: "feat/login" }]]),
    settings: { accounts: [{ name: "Work", config_dir: "/cfg/work" }] },
  };
  expect(recordNames(state as unknown as Parameters<typeof recordNames>[0]).sort()).toEqual(["/cfg/work", "/work/octo", "Fix login", "Main console", "Octo", "Work", "feat/login", "git@host:me/octo.git", "zsh - fix"]);
});

const environment = { version: "1.2.3", platform: "tauri" as const, userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", language: "en" };
const facts = (failures: IssueFacts["failures"], extra: Partial<IssueFacts> = {}): IssueFacts => ({ failures, environment, context, ...extra });
const BASE = "https://github.com/owner/repo/issues/new";
const parse = (url: string) => {
  expect(url.startsWith(`${BASE}?`)).toBe(true);
  const params = new URL(url).searchParams;
  return { title: params.get("title")!, body: params.get("body")! };
};

it("describes a thrown error with its sanitized message and stack, and the environment", () => {
  const error = new Error("Cannot open /Users/alice/Secret Project/notes.md");
  error.stack = "Error: Cannot open /Users/alice/Secret Project/notes.md\n    at run (tauri://localhost/assets/index-1a2b.js:3:4)";
  const { title, body } = parse(issueReportUrl(BASE, facts([{ error }])));
  expect(title).toBe("Unexpected error: Cannot open <path>");
  expect(body).toContain("### What happened");
  expect(body).toContain(`- ${appConfig.name}: 1.2.3\n- Platform: desktop app\n- OS: macOS 10.15.7\n- Language: en`);
  expect(body).toContain("- Kind: unexpected");
  expect(body).toContain("```\nError: Cannot open <path>\n    at run (tauri://localhost/assets/index-1a2b.js:3:4)\n```");
  expect(body).not.toContain("alice");
  expect(body).not.toContain("Secret");
});

it("gives an actionable failure only its kind, code and parameter names, never their values, its message or a title of its own", () => {
  const locked = new DaemonRequestError("Localized wording about Secret Project", "session_locked", { path: "/Users/alice/p", name: "Secret Project" });
  const plain = new ActionableError("Could not write /Users/alice/Secret Project", "clipboard");
  const { title, body } = parse(issueReportUrl(BASE, facts([{ error: locked }, { error: plain }])));
  expect(title).toBe("Unexpected error");
  expect(body).toContain("- Code: `session_locked`");
  expect(body).toContain("- Parameters: `name`, `path`");
  expect(body).toContain("- Kind: actionable (clipboard)");
  for (const leaked of ["alice", "Secret", "Localized", "```"]) expect(body).not.toContain(leaked);
});

it("describes a JavaScript error the daemon client wrapped as unexpected_error by its stack, led by its message", () => {
  const error = new DaemonRequestError("Cannot read x of undefined", "unexpected_error", {});
  error.stack = "    at f (tauri://localhost/assets/index-ab12.js:1:2)";
  const { title, body } = parse(issueReportUrl(BASE, facts([{ error }])));
  expect(title).toBe("Unexpected error: Cannot read x of undefined");
  expect(body).toContain("```\nCannot read x of undefined\n    at f (tauri://localhost/assets/index-ab12.js:1:2)\n```");
});

it("adds the raw text an alert shows for an unexpected coded failure, but not for an actionable one", () => {
  const detail = "Unexpected state in /Users/alice/Secret Project";
  const unexpected = new DaemonRequestError(detail, "internal_error", { detail });
  const { title, body } = parse(issueReportUrl(BASE, facts([{ error: unexpected }])));
  expect(title).toBe("Unexpected error: Unexpected state in <path>");
  expect(parse(issueReportUrl(BASE, facts([{ error: new DaemonRequestError("", "internal_error", {}) }]))).title).toBe("Unexpected error: internal_error");
  expect(body).toContain("- Parameters: `detail`");
  expect(body).toContain("```\nUnexpected state in <path>\n```");
  const actionable = new DaemonRequestError("No space left on device (os error 28)", "internal_error", { detail: "disk /Users/alice" });
  const other = parse(issueReportUrl(BASE, facts([{ error: actionable }]))).body;
  expect(other).toContain("- Kind: actionable");
  expect(other).not.toContain("```");
});

it("deduplicates failures and falls back to a bare title when there is nothing to say", () => {
  const error = new Error("same");
  error.stack = "Error: same\n    at a (x.js:1:1)";
  const { body } = parse(issueReportUrl(BASE, facts([{ error }, { error }])));
  expect(body.match(/- Kind:/g)).toHaveLength(1);
  const empty = parse(issueReportUrl(BASE, facts([])));
  expect(empty.title).toBe("Unexpected error");
  expect(empty.body).toContain("No error details were captured.");
});

it("keeps the address within the cap by cutting each stack from its tail, the title whole", () => {
  const stack = Array.from({ length: 400 }, (_, index) => `    at frame${index} (tauri://localhost/assets/index-ab12.js:${index}:99)`).join("\n");
  const error = Object.assign(new Error("deep"), { stack: `Error: deep\n${stack}` });
  const url = issueReportUrl(BASE, facts([{ error }]));
  expect(url.length).toBeLessThanOrEqual(7000);
  const { title, body } = parse(url);
  expect(title).toBe("Unexpected error: deep");
  expect(body).toContain("Error: deep\n    at frame0 ");
  expect(body).toContain("\n… (truncated)\n```");
  expect(body).not.toContain("frame399");
});

it("fences a diagnostic that itself holds a code fence with a longer one", () => {
  const error = Object.assign(new Error("x"), { stack: "Error: x\n```js\nboom\n```" });
  expect(parse(issueReportUrl(BASE, facts([{ error }]))).body).toContain("````\nError: x\n```js\nboom\n```\n````");
});
