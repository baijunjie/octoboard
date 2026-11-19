// What the fixture daemon answers a content search (`search_project_content`) with: the matching lines of the project's
// text files, found as the daemon finds them (see "Searching file contents" in `apps/daemon/PROTOCOL.md`), so the lines
// a result shows and the file a preview reads always agree, and the refusals a pattern or a file mask earns.
import type { ClientRequestBody, ContentMatch, ContentSearchStop } from "../protocol";
import type { ContentSearchEvent } from "../store";
import { refuseUnfillable } from "./banner";
import type { FixtureError } from "./sampleFiles";

type SearchBody = Extract<ClientRequestBody, { type: "search_project_content" }>;

/** The most characters of a line a match carries (`text` is clipped to about this many around the first match). */
const CLIP_CHARS = 250;

/** The most lines in one `project_content_matches` batch. */
const BATCH_LINES = 500;

/** How a search ends, as a scenario stages it: `"searching"` sends no event after the `ack`, leaving the search running;
 * `stoppedBy` ends it at that bound, incomplete. */
export type ContentSearchStaging = "searching" | { stoppedBy: ContentSearchStop };

/** The files a search reads, as the fixture daemon found them in scope: wire path and text. */
export type SearchedFile = readonly [path: string, text: string];

/** Escapes `text` to be a regular expression matching it literally. */
const literal = (text: string) => text.replace(/[\\^$.*+?()[\]{}|]/g, "\\$&");

/** The daemon's refusal params for a pattern the regex crate refuses, for the faults the fixture can tell as it does:
 * the `cause`, the UTF-16 `offset` of the one character the fault is about, and the crate's own `detail` (the pointer
 * under the pattern sits at that character). Any other fault is refused as unfillable: the reading is the crate's, not
 * the browser's. */
function regexRefusal(pattern: string, fault: string): { cause: string; offset: string; detail: string } {
  const refusal = (cause: string, at: number, error: string) => ({ cause, offset: String(at), detail: `regex parse error:\n    ${pattern}\n    ${" ".repeat(at)}^\nerror: ${error}` });
  const open: number[] = [];
  let unopened = -1;
  let classAt = -1;
  for (let i = 0; i < pattern.length; i++) {
    if (pattern[i] === "\\") i++;
    else if (classAt !== -1) {
      if (pattern[i] === "]" && i > classAt + 1) classAt = -1;
    } else if (pattern[i] === "[") classAt = i;
    else if (pattern[i] === "(") open.push(i);
    else if (pattern[i] === ")" && open.pop() === undefined && unopened === -1) unopened = i;
  }
  if (fault.includes("Unterminated character class") && classAt !== -1) return refusal("unclosed_class", classAt, "unclosed character class");
  if (fault.includes("Unterminated group") && open.length > 0) return refusal("unclosed_group", open[open.length - 1]!, "unclosed group");
  if (fault.includes("Unmatched ')'") && unopened !== -1) return refusal("unopened_group", unopened, "unopened group");
  return refuseUnfillable(`the fixture cannot tell the regex crate's refusal of ${JSON.stringify(pattern)}: ${fault}`);
}

/** A glob the glob crate refuses: why, as the daemon's `cause`, and the crate's own explanation. */
class GlobRefusal extends Error {
  constructor(readonly reason: "unclosed_class" | "unclosed_alternation", detail: string) {
    super(detail);
  }
}

/** The expression a query searches with, or the refusal of one that does not compile. */
function matcherOf(body: SearchBody, fail: (error: FixtureError) => never): RegExp {
  const source = body.regex === true ? body.query : literal(body.query);
  const wrapped = body.whole_word === true ? `(?<!\\w)(?:${source})(?!\\w)` : source;
  try {
    return new RegExp(wrapped, body.match_case === true ? "g" : "gi");
  } catch (error) {
    const params = regexRefusal(body.query, error instanceof Error ? error.message : String(error));
    return fail({ code: "invalid_search_pattern", params, message: `the search pattern is not a valid regular expression: ${params.detail}` });
  }
}

/** One glob of a file mask as an expression over a name or a path: `*` and `?` stay within a path segment, `**` crosses
 * them, `[...]` is a class and `{a,b}` a choice (a mask is split at its commas first, as the daemon splits it, so a comma
 * inside braces is never part of one glob). Throws a `GlobRefusal` for a glob the glob crate refuses. */
function globExpression(glob: string): RegExp {
  let source = "";
  for (let i = 0; i < glob.length; i++) {
    const char = glob[i]!;
    if (char === "*") {
      if (glob[i + 1] === "*") {
        source += ".*";
        i++;
        if (glob[i + 1] === "/") i++;
      } else source += "[^/]*";
    } else if (char === "?") source += "[^/]";
    else if (char === "[") {
      const close = glob.indexOf("]", i + 2);
      if (close === -1) throw new GlobRefusal("unclosed_class", "unclosed character class; missing ']'");
      source += `[${glob[i + 1] === "!" ? "^" : ""}${glob.slice(glob[i + 1] === "!" ? i + 2 : i + 1, close)}]`;
      i = close;
    } else if (char === "{") {
      const close = glob.indexOf("}", i);
      if (close === -1) throw new GlobRefusal("unclosed_alternation", "unclosed alternate group; missing '}' (maybe escape '{' with '[{]'?)");
      source += `(?:${glob.slice(i + 1, close).split(",").map(literal).join("|")})`;
      i = close;
    } else if (char === "\\" && i + 1 < glob.length) source += literal(glob[++i]!);
    else source += literal(char);
  }
  return new RegExp(`^${source}$`);
}

/** The test a file mask makes of a path: comma-separated globs a file must match one of, a leading `!` leaving out what
 * a glob matches; a glob with no `/` is matched against the file's name, one with a `/` against its path. */
function maskOf(mask: string | undefined, fail: (error: FixtureError) => never): (path: string) => boolean {
  const globs = (mask ?? "").split(",").map((glob) => glob.trim()).filter((glob) => glob !== "").map((glob) => {
    const excluded = glob.startsWith("!");
    const pattern = (excluded ? glob.slice(1).trim() : glob);
    const byPath = pattern.includes("/");
    try {
      return { excluded, byPath, expression: globExpression(pattern.startsWith("/") ? pattern.slice(1) : pattern) };
    } catch (error) {
      if (!(error instanceof GlobRefusal)) throw error;
      return fail({ code: "invalid_file_mask", params: { glob, cause: error.reason, detail: error.message }, message: `${glob} is not a valid file mask: ${error.message}` });
    }
  });
  const includes = globs.filter((glob) => !glob.excluded);
  return (path) => {
    const name = path.slice(path.lastIndexOf("/") + 1);
    const hits = (glob: (typeof globs)[number]) => glob.expression.test(glob.byPath ? path : name);
    if (globs.some((glob) => glob.excluded && hits(glob))) return false;
    return includes.length === 0 || includes.some(hits);
  };
}

/** A matching line as the daemon sends it: clipped to about `CLIP_CHARS` around its first match (a third of them before
 * it), and its ranges in the whole line's UTF-16 offsets. */
function matchOf(path: string, line: number, whole: string, ranges: ContentMatch["ranges"]): ContentMatch {
  if (whole.length <= CLIP_CHARS) return { path, line, text: whole, text_start: 0, ranges, clipped_end: false };
  const start = Math.max(0, ranges[0]!.start - Math.floor(CLIP_CHARS / 3));
  const text = whole.slice(start, start + CLIP_CHARS);
  return { path, line, text, text_start: start, ranges, clipped_end: start + CLIP_CHARS < whole.length };
}

/** What the daemon sends for the search `body` over `files` (the text files in its scope, already left out of what the
 * search skips): the batches of lines, then the end. A file mask or a pattern that does not compile is refused with
 * `fail`, before anything is found. `incomplete` is whether a folder in scope could not be read in full. */
export function searchContent(
  files: readonly SearchedFile[],
  body: SearchBody,
  { incomplete, stoppedBy, fail }: { incomplete: boolean; stoppedBy?: ContentSearchStop; fail: (error: FixtureError) => never },
): ContentSearchEvent[] {
  const matcher = matcherOf(body, fail);
  const admits = maskOf(body.file_mask, fail);
  const found: ContentMatch[] = [];
  let matches = 0;
  const filesWithMatches = new Set<string>();
  if (body.query !== "") {
    for (const [path, text] of [...files].sort(([a], [b]) => (a < b ? -1 : 1))) {
      // A file with a NUL byte is binary and yields nothing.
      if (!admits(path) || text.includes("\0")) continue;
      text.split("\n").forEach((line, index) => {
        const ranges: ContentMatch["ranges"] = [];
        matcher.lastIndex = 0;
        for (let hit = matcher.exec(line); hit !== null; hit = matcher.exec(line)) {
          // A match is never empty.
          if (hit[0] === "") matcher.lastIndex++;
          else ranges.push({ start: hit.index, end: hit.index + hit[0].length });
        }
        if (ranges.length === 0) return;
        found.push(matchOf(path, index + 1, line.replace(/\r$/, ""), ranges));
        matches += ranges.length;
        filesWithMatches.add(path);
      });
    }
  }
  const events: ContentSearchEvent[] = [];
  for (let at = 0; at < found.length; at += BATCH_LINES) {
    events.push({ type: "project_content_matches", search: body.search, project: body.project, matches: found.slice(at, at + BATCH_LINES) });
  }
  events.push({
    type: "project_content_search_ended",
    search: body.search,
    project: body.project,
    matches,
    files: filesWithMatches.size,
    complete: !incomplete && stoppedBy === undefined,
    ...(stoppedBy !== undefined && { stopped_by: stoppedBy }),
  });
  return events;
}
