import { appConfig } from "./appConfig";
import { DaemonRequestError } from "./daemon-client";
import { diagnosticText, errorDiagnostic, rawFailureText } from "./errorText";
import type { ReportableFailure } from "./errorAlerts";
import { classifyError, type ErrorClassification } from "./errorRecovery";
import type { State } from "./store";

/** GitHub rejects a request line much beyond this; the prefilled address stays well inside it. */
const MAX_URL_LENGTH = 7000;
const MAX_TITLE_SUMMARY = 80;
const MAX_PARAM_KEYS = 20;
const MIN_NAME_LENGTH = 2;
// A text longer than this is cut before it is sanitized, the patterns costing more than linear on a long run.
const MAX_TEXT_LENGTH = 16000;
const TRUNCATED = "… (truncated)";

/** What the sanitizer knows to be the user's: the names of records they made, and their home directory. */
export interface SanitizeContext {
  names: readonly string[];
  homeDir?: string | null;
}

export interface IssueEnvironment {
  version?: string;
  platform?: "tauri" | "browser";
  userAgent?: string;
  language?: string;
}

export interface IssueFacts {
  failures: readonly ReportableFailure[];
  environment: IssueEnvironment;
  context: SanitizeContext;
}

/** The names of the records the user made, which an error text may quote: collected only for a failure being
 * reported, since it walks every record. */
export function recordNames(state: Pick<State, "consoles" | "projects" | "sessions" | "shellTerminals" | "gitStatuses" | "settings">): string[] {
  const names = new Set<string>();
  for (const record of state.consoles.values()) names.add(record.name);
  for (const record of state.projects.values()) {
    names.add(record.name);
    names.add(record.path);
    if (record.remote_url) names.add(record.remote_url);
  }
  for (const record of state.sessions.values()) {
    names.add(record.title);
    if (record.terminal_title) names.add(record.terminal_title);
  }
  for (const record of state.shellTerminals.values()) names.add(record.title);
  for (const record of state.gitStatuses.values()) if (record.branch) names.add(record.branch);
  for (const account of state.settings.accounts) {
    names.add(account.name);
    names.add(account.config_dir);
  }
  return [...names].filter((name) => name.trim().length >= MIN_NAME_LENGTH);
}

const PLACEHOLDER = String.raw`<(?:name|path|url|email|ip|id|redacted)>`;
// A path segment, which may hold a placeholder: names are replaced before paths, being able to hold spaces.
const PATH_CHARS = String.raw`(?:${PLACEHOLDER}|[^\s'"\`:;,)<>])`;
const HEX_GROUP = "[0-9a-f]{1,4}";
const SECRET_WORD = String.raw`(?:token|secret|password|passwd|pwd|api[_-]?key|access[_-]?key|private[_-]?key|authorization|credential|session[_-]?id)`;

// The generic home directories, as sources so the whole-text patterns and the one inside a URL's path stay one.
const WINDOWS_HOME_SOURCE = String.raw`[A-Za-z]:[\\/]Users[\\/][^\\/\s'"\`:;,)<>]+`;
const POSIX_HOME_SOURCE = String.raw`\/(?:Users|home)\/[^/\s'"\`:;,)<>]+`;
const WINDOWS_HOME_PATTERN = new RegExp(String.raw`(?<![A-Za-z0-9])${WINDOWS_HOME_SOURCE}`, "gi");
const POSIX_HOME_PATTERN = new RegExp(String.raw`(?<![\w.~-])${POSIX_HOME_SOURCE}`, "g");
const URL_PATTERN = new RegExp(String.raw`\b[a-z][a-z0-9+.-]*:\/\/(?:${PLACEHOLDER}|[^\s'"\`<>)])+`, "gi");
const SCP_PATTERN = /(?<![\w.%+-])[A-Za-z0-9._-]+@[A-Za-z0-9.-]+:[~\w./-]+/g;
const PATH_PATTERNS = [
  // What follows a path already put in, being the rest of a path whose directory held spaces.
  new RegExp(String.raw`<path>(?:[\\/]${PATH_CHARS}+)+`, "g"),
  new RegExp(String.raw`(?<![\w.:/~-])(?:~(?:\/${PATH_CHARS}+)+|(?:\/${PATH_CHARS}+){2,})`, "g"),
  new RegExp(String.raw`(?<![A-Za-z0-9])[A-Za-z]:[\\/]${PATH_CHARS}*`, "g"),
  new RegExp(String.raw`(?<![\w.:/~-])~(?:\\${PATH_CHARS}+)+`, "g"),
  new RegExp(String.raw`\\\\${PATH_CHARS}+(?:\\${PATH_CHARS}+)*`, "g"),
];
const EMAIL_PATTERN = /(?<![A-Za-z0-9._%+-])[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+/g;
const IPV4_PATTERN = /(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w]|\.\d)/g;
const IPV6_PATTERN = new RegExp(String.raw`(?<![\w:])(?:(?:${HEX_GROUP}:){7}${HEX_GROUP}|(?:${HEX_GROUP}:){1,7}:(?:${HEX_GROUP}(?::${HEX_GROUP}){0,6})?|::(?:${HEX_GROUP}(?::${HEX_GROUP}){0,6})?)(?![\w:])`, "gi");
const UUID_PATTERN = /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi;
const SECRET_PATTERNS = [
  /\b(?:Bearer|Basic)\s+[A-Za-z0-9._~+/=-]{8,}/g,
  /\b(?:sk|pk|rk)-[A-Za-z0-9_-]{10,}/g,
  /\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[abprs]-[A-Za-z0-9-]{10,})/g,
];
// A key that holds a secret word anywhere in its name (`DB_PASSWORD`, `client_secret`), and the value it is given.
const SECRET_ASSIGNMENT = new RegExp(String.raw`(?<![A-Za-z0-9_.-])([A-Za-z0-9_.-]*${SECRET_WORD}[A-Za-z0-9_.-]*["']?\s*[=:]\s*)["']?[^\s"',;]+["']?`, "gi");
const COOKIE_PATTERN = /\b((?:set-)?cookie\s*[=:]\s*)[^\n]+/gi;
// A letter-and-digit run this long is a key or a hash far more often than an identifier of the code.
const LONG_TOKEN_PATTERN = /(?<![A-Za-z0-9_+-])(?=[A-Za-z0-9_+-]*\d)(?=[A-Za-z0-9_+-]*[A-Za-z])[A-Za-z0-9_+-]{20,}={0,2}(?![A-Za-z0-9_+-])/g;
const PLACEHOLDER_PATTERN = new RegExp(PLACEHOLDER, "g");

// Where the application's own scripts are served from, whose stack frames say where in the bundle a failure was.
const APP_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]", "tauri.localhost", "asset.localhost", "ipc.localhost"]);

function isLoopbackIp(address: string): boolean {
  return address === "::1" || address === "::" || address.startsWith("127.") || address === "0.0.0.0";
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** A URL in a text: the application's own is kept without its query and fragment, the line and column a stack frame
 * ends in staying and a home directory in its path (a dev server's `/@fs/Users/bob/...`) left out; a local file is a
 * path; any other can name a private repository or an internal host, so it is replaced whole. */
function sanitizeUrl(url: string, home: string | undefined): string {
  const [, scheme, rest] = /^([a-z][a-z0-9+.-]*):\/\/(.*)$/is.exec(url)!;
  if (scheme.toLowerCase() === "file") return "<path>";
  const authorityEnd = rest.search(/[/?#]/);
  const authority = authorityEnd < 0 ? rest : rest.slice(0, authorityEnd);
  const host = authority.replace(/^[^@]*@/, "").replace(/:\d+$/, "").toLowerCase();
  if (authority.includes("@") || !APP_HOSTS.has(host)) return "<url>";
  const after = authorityEnd < 0 ? "" : rest.slice(authorityEnd);
  const [, path = "", position = ""] = /^([^?#]*)(?:[?#].*?)?((?::\d+){0,2})$/s.exec(after) ?? [];
  return `${scheme}://${authority}${collapseHome(path, home)}${position}`;
}

/** `path` of a URL with the user's home directory, known or of the generic shapes, collapsed to `~`. */
function collapseHome(path: string, home: string | undefined): string {
  const known = home ? path.replace(homePattern(home, false), "/~") : path;
  return known.replace(new RegExp(WINDOWS_HOME_SOURCE, "gi"), "~").replace(new RegExp(POSIX_HOME_SOURCE, "g"), "/~");
}

/** Whether a name is itself a path (a project's or an account's directory), which is then replaced, as `<path>`, before
 * the home directory is collapsed out of it. */
function isPathName(name: string): boolean {
  return /^(?:[/~]|[A-Za-z]:[\\/])/.test(name);
}

/** `names` replaced in `text` by `placeholder` wherever they stand alone, longest first, outside the placeholders
 * already put in. A name stands alone when no letter or digit flanks it; a path name is a whole directory, so only
 * what may end a path follows it (the end, whitespace, a separator, a quote or punctuation, a `.` that is not the start
 * of an extension), and a sibling that merely starts with it (`proj-old`, `proj+old`, `proj(1)`) is not it. */
function replaceNames(text: string, names: readonly string[], placeholder: string, path = false): string {
  const sorted = [...new Set(names)].filter((name) => name.length >= MIN_NAME_LENGTH).sort((a, b) => b.length - a.length);
  if (sorted.length === 0) return text;
  const pattern = new RegExp(String.raw`(?<![A-Za-z0-9])(?:${sorted.map(escapeRegExp).join("|")})${path ? String.raw`(?=$|[\s/\\'"\`:,;)&?#]|\.(?![A-Za-z0-9]))` : "(?![A-Za-z0-9])"}`, "g");
  const placeholders = text.match(PLACEHOLDER_PATTERN) ?? [];
  let index = 0;
  return text.split(PLACEHOLDER_PATTERN).map((part) => part.replace(pattern, placeholder)).reduce((result, part) => `${result}${part}${placeholders[index++] ?? ""}`, "");
}

/** The home directory as a pattern that ends at a path segment boundary, so a longer name sharing its start stays. */
function homePattern(home: string, flanked = true): RegExp {
  return new RegExp(String.raw`${flanked ? "(?<![A-Za-z0-9])" : ""}${escapeRegExp(home.replace(/[\\/]+$/, ""))}(?![^\s'"\`:;,)<>/\\])`, "g");
}

/** A text from an error with everything that could identify the user or their machine replaced by a placeholder: first
 * the names of their records that are paths (which a collapsed home directory would no longer match), then their home
 * directory, URLs, email addresses, secrets, addresses and identifiers, then their other record names, then every other
 * absolute path. The addresses of the application's own scripts in a stack frame stay, being the useful part of a
 * stack. A path with a space in it is only redacted as far as its first space, unless a record name covers it. */
export function sanitizeReportText(text: string, context: SanitizeContext): string {
  let result = text.length > MAX_TEXT_LENGTH ? `${head(text, MAX_TEXT_LENGTH)}\n${TRUNCATED}` : text;
  result = result.replace(/[\u2066-\u2069]/g, "");
  // The home directory shortens a path name to its `~` form too.
  const home = context.homeDir && context.homeDir.length > 1 ? context.homeDir.replace(/[\\/]+$/, "") : undefined;
  const pathNames = context.names.filter(isPathName).flatMap((name) => (home && name.startsWith(home) ? [name, `~${name.slice(home.length)}`] : [name]));
  result = replaceNames(result, pathNames, "<path>", true);
  result = result.replace(WINDOWS_HOME_PATTERN, "~");
  if (home) result = result.replace(homePattern(home), "~");
  result = result.replace(POSIX_HOME_PATTERN, "~");
  result = result.replace(URL_PATTERN, (url) => sanitizeUrl(url, home)).replace(SCP_PATTERN, "<url>");
  result = result.replace(EMAIL_PATTERN, "<email>");
  for (const pattern of SECRET_PATTERNS) result = result.replace(pattern, "<redacted>");
  result = result.replace(COOKIE_PATTERN, "$1<redacted>").replace(SECRET_ASSIGNMENT, "$1<redacted>");
  result = result.replace(UUID_PATTERN, "<id>").replace(LONG_TOKEN_PATTERN, "<redacted>");
  result = result.replace(IPV4_PATTERN, (address) => (isLoopbackIp(address) || address.split(".").some((part) => Number(part) > 255) ? address : "<ip>"));
  result = result.replace(IPV6_PATTERN, (address) => (isLoopbackIp(address) ? address : "<ip>"));
  result = replaceNames(result, context.names.filter((name) => !isPathName(name)), "<name>");
  for (const pattern of PATH_PATTERNS) result = result.replace(pattern, "<path>");
  return result;
}

/** The operating system a user agent names, as coarsely as it does and without anything else of the device. */
export function osFromUserAgent(userAgent: string | undefined): string | undefined {
  if (!userAgent) return undefined;
  const version = (match: RegExpExecArray | null) => match?.[1].replace(/_/g, ".");
  const windows = version(/Windows NT ([\d.]+)/.exec(userAgent));
  if (windows) return `Windows NT ${windows}`;
  const ios = version(/(?:iPhone|CPU) OS ([\d_]+)/.exec(userAgent));
  if (ios) return `iOS ${ios}`;
  const mac = version(/Mac OS X ([\d_.]+)/.exec(userAgent));
  if (mac) return `macOS ${mac}`;
  const android = version(/Android ([\d.]+)/.exec(userAgent));
  if (android) return `Android ${android}`;
  if (/CrOS/.test(userAgent)) return "ChromeOS";
  if (/Linux|X11/.test(userAgent)) return "Linux";
  return undefined;
}

function firstLine(text: string): string {
  return text.split("\n", 1)[0].trim();
}

/** The first `limit` characters of `text`, never half of a surrogate pair. */
function head(text: string, limit: number): string {
  const cut = text.slice(0, limit);
  return /[\ud800-\udbff]$/.test(cut) ? cut.slice(0, -1) : cut;
}

/** What an unexpected failure says of itself in a report: for one the daemon coded, the raw text its alert shows under
 * "Error details" (see `rawFailureText`); for any other error its stack, led by its name and message. */
function diagnosticOf(error: unknown): string | undefined {
  if (error instanceof DaemonRequestError) return rawFailureText(error);
  if (error instanceof Error) return errorDiagnostic(error);
  return diagnosticText(error);
}

/** What the title says of a failure: the daemon's code, except for the generic codes that say nothing, which give way to
 * the first line of the raw text the body shows; else the first line of a thrown error's message. */
function summaryOf(error: unknown, context: SanitizeContext): string | undefined {
  const coded = error instanceof DaemonRequestError ? error : undefined;
  const generic = coded?.code === "internal_error" || coded?.code === "unexpected_error";
  if (coded && !generic) return coded.code;
  const text = coded ? rawFailureText(coded) : error instanceof Error ? error.message || error.name : diagnosticText(error);
  const line = text ? firstLine(sanitizeReportText(text, context)) : "";
  if (!line) return coded?.code;
  return line.length > MAX_TITLE_SUMMARY ? `${head(line, MAX_TITLE_SUMMARY - 1)}…` : line;
}

interface ReportedFailure {
  kind: string;
  code?: string;
  paramKeys: string[];
  diagnostic?: string;
}

/** How a failure is classified: as its caller says if it does, else by its error. */
function classificationOf({ error, recovery, unexpected }: ReportableFailure): ErrorClassification {
  return unexpected ? { kind: "unexpected" } : classifyError(error, recovery);
}

function reportedFailures({ failures, context }: IssueFacts): ReportedFailure[] {
  const seen = new Set<string>();
  const reported: ReportedFailure[] = [];
  for (const failure of failures) {
    const { error } = failure;
    const classification = classificationOf(failure);
    const coded = error instanceof DaemonRequestError ? error : undefined;
    // An actionable failure's message is worded for the user and its params are theirs: only what kind it is.
    const diagnostic = classification.kind === "unexpected" ? diagnosticOf(error) : undefined;
    const entry: ReportedFailure = {
      kind: classification.kind === "unexpected" ? "unexpected" : `actionable (${classification.recovery})`,
      code: coded?.code,
      paramKeys: coded ? Object.keys(coded.params).sort().slice(0, MAX_PARAM_KEYS) : [],
      diagnostic: diagnostic ? sanitizeReportText(diagnostic, context).trim() || undefined : undefined,
    };
    const signature = JSON.stringify(entry);
    if (seen.has(signature)) continue;
    seen.add(signature);
    reported.push(entry);
  }
  return reported;
}

/** `text` as a code block whose fence is longer than any run of backticks in it. */
function fenced(text: string): string {
  const longest = Math.max(0, ...[...text.matchAll(/`+/g)].map((run) => run[0].length));
  const fence = "`".repeat(Math.max(3, longest + 1));
  return `${fence}\n${text}\n${fence}`;
}

function titleOf({ failures, context }: IssueFacts): string {
  const main = failures.find((failure) => classificationOf(failure).kind === "unexpected");
  const summary = main && summaryOf(main.error, context);
  return summary ? `Unexpected error: ${summary}` : "Unexpected error";
}

function bodyOf(facts: IssueFacts, diagnostics: ReportedFailure[], diagnosticLimit: number): string {
  const { environment } = facts;
  const os = osFromUserAgent(environment.userAgent);
  const environmentLines = [
    environment.version && `- ${appConfig.name}: ${environment.version}`,
    environment.platform && `- Platform: ${environment.platform === "tauri" ? "desktop app" : "browser"}`,
    os && `- OS: ${os}`,
    environment.language && `- Language: ${environment.language}`,
  ].filter((line): line is string => !!line);
  const errors = diagnostics.map((failure) => {
    const lines = [
      `- Kind: ${failure.kind}`,
      failure.code && `- Code: \`${failure.code}\``,
      failure.paramKeys.length > 0 && `- Parameters: ${failure.paramKeys.map((key) => `\`${key}\``).join(", ")}`,
    ].filter((line): line is string => !!line);
    if (failure.diagnostic) {
      const shown = failure.diagnostic.length > diagnosticLimit ? `${head(failure.diagnostic, diagnosticLimit).trimEnd()}\n${TRUNCATED}` : failure.diagnostic;
      lines.push("", fenced(shown));
    }
    return lines.join("\n");
  });
  return [
    "### What happened",
    "<!-- Describe what you were doing when this happened, and what you expected instead. -->",
    "The details below were filled in automatically, with personal information removed. Please check them before submitting.",
    "### Environment",
    environmentLines.length > 0 ? environmentLines.join("\n") : "Unknown",
    "### Error",
    errors.length > 0 ? errors.join("\n\n") : "No error details were captured.",
  ].join("\n\n");
}

function urlWith(baseUrl: string, title: string, body: string): string {
  const query = new URLSearchParams({ title, body }).toString();
  return `${baseUrl}${baseUrl.includes("?") ? "&" : "?"}${query}`;
}

/** `baseUrl` with the title and body of a new issue prefilled from `facts`: what the failures and the environment
 * are, as far as they can be said without personal information (see `sanitizeReportText`). Where the address would
 * exceed what GitHub accepts, the diagnostic texts are cut to the longest head that fits, each ending in a note of
 * the cut; the title is always whole. */
export function issueReportUrl(baseUrl: string, facts: IssueFacts): string {
  const title = titleOf(facts);
  const failures = reportedFailures(facts);
  const full = urlWith(baseUrl, title, bodyOf(facts, failures, Infinity));
  if (full.length <= MAX_URL_LENGTH) return full;
  let low = 0;
  let high = Math.max(0, ...failures.map((failure) => failure.diagnostic?.length ?? 0));
  // The address grows with the limit, so the largest limit that fits is found by bisection.
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (urlWith(baseUrl, title, bodyOf(facts, failures, middle)).length <= MAX_URL_LENGTH) low = middle;
    else high = middle - 1;
  }
  return urlWith(baseUrl, title, bodyOf(facts, failures, low));
}
