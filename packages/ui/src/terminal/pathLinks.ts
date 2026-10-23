import type { IBufferRange, ILink, ILinkProvider, Terminal } from "@xterm/xterm";

import type { PathLinkKind, PathLinkView } from "../protocol";
import { isPrimaryModifier } from "../useWindowShortcut";
import {
  cachedRows,
  crossesJoin,
  indentOf,
  type LineChain,
  type LineLayout,
  lineCells,
  lineLength,
  type LineRow,
  type LinkSegment,
  type LineText,
  lineText,
  logicalLinesIn,
  matchCells,
  mergeSegments,
  terminalRowReader,
  type TerminalRow,
  textEnd,
} from "./terminalLines";
import { type ExtraLinkSegments, terminalLinkHandler } from "./terminalLinks";

/** The most paths one probe may ask about; the daemon refuses more. */
const MAX_PROBED_PATHS = 64;
/** How long wanted paths gather before they are asked about, so that a screen of output is one
 * request, and how many requests may be out at once. */
const PROBE_DELAY_MS = 120;
const MAX_PROBES_IN_FLIGHT = 2;
/** The longest text taken as a path. */
const MAX_PATH_CHARS = 1024;
/** How long an answer is trusted before the path is asked about again: a file the agent names
 * before it writes it, or one that goes away, is corrected within this. */
const FOUND_TTL_MS = 30_000;
const NONE_TTL_MS = 10_000;
/** The most answers kept; past it the oldest half goes, so a long session cannot grow it without bound. */
const MAX_ENTRIES = 2000;

/** The characters a path is made of in running text: not blanks, quotes, brackets, pipes or
 * full-width punctuation, which end one. */
const PATH_CHAR = String.raw`[^\s"'\`<>()\[\]{}|\\^*?\u3000-\u303f\uff00-\uffef]`;
/** What may stand right before a path: the start of the text, a blank, or an opening quote or
 * bracket, a comma, a semicolon or an equals sign. */
const PATH_START = String.raw`(?<=^|[\s"'\`(\[<{=,;\u3000-\u303f\uff00-\uffef])`;
const PATH_TOKEN = new RegExp(`${PATH_START}${PATH_CHAR}+`, "g");
const TRAILING_PUNCTUATION = /[.,;:!?]+$/;
const LINE_SUFFIX = /:\d+(?::\d+)?$/;

/** What the daemon answered for a path: what it names and, for a file the file viewer can show (or a
 * folder's README or index file), where to read it from. */
export interface PathAnswer {
  kind: PathLinkKind;
  view?: PathLinkView;
  /** The file could be launched by the system: it is for the viewer, never for the host to open. */
  viewerOnly?: boolean;
}

/** A path as the user clicked it: whether it is a file or a folder, its line, if the text gave one,
 * and its view, if it has one. */
export interface PathLinkTarget {
  path: string;
  kind?: Exclude<PathLinkKind, "none">;
  line?: number;
  view?: PathLinkView;
  viewerOnly?: boolean;
}

/** A path found in a run of text: the characters `start` to `end` (exclusive) read as a path with
 * an optional `:line[:col]` after it (or a compiler's `(line,col)`), and `path` without that
 * suffix. */
export interface PathCandidate {
  start: number;
  end: number;
  path: string;
  line?: number;
}

const COMPILER_POSITION = /^\((\d+)(?:,\d+)?\)/;

/**
 * The paths a run of text appears to mention: absolute (`/…`), home-relative (`~/…`) and relative
 * with a directory in them (`src/a.ts`, `./a`, `../a`), or with a `:line` after a file name
 * (`a.ts:42`), each optionally mentioned with a leading `@`. A trailing `.`, `,`, `;`, `:`, `!` or
 * `?` is not part of one, and an address (`https://…`) is not one. Whether a path exists is the
 * daemon's to say; this only finds the text worth asking about.
 */
export function pathCandidates(text: string): PathCandidate[] {
  const found: PathCandidate[] = [];
  for (const match of text.matchAll(PATH_TOKEN)) {
    // A path that goes on into a bracket is a route pattern (`app/[id]/page.tsx`), not the part of
    // it before the bracket. A parenthesis after a name is a compiler's position (`a.ts(12,5)`).
    const next = text[match.index + match[0].length];
    if (next === "[" || next === "{" || (next === "(" && match[0].endsWith("/"))) continue;
    // An `@` in front is a mention of the path (`@src/a.ts`), not part of it.
    const mention = match[0].startsWith("@") ? 1 : 0;
    const token = match[0].slice(mention).replace(TRAILING_PUNCTUATION, "");
    if (token.length === 0 || token.length > MAX_PATH_CHARS || token.includes("://") || token.startsWith("//")) continue;
    const suffix = LINE_SUFFIX.exec(token);
    const path = suffix ? token.slice(0, suffix.index) : token;
    if (!isPathLike(path, suffix !== null)) continue;
    const compiler = next === "(" ? COMPILER_POSITION.exec(text.slice(match.index + match[0].length)) : null;
    const line = Number(suffix ? suffix[0].split(":")[1] : compiler?.[1]);
    found.push({
      start: match.index + mention,
      end: match.index + mention + token.length,
      path,
      ...(line > 0 && { line }),
    });
  }
  return found;
}

function isPathLike(path: string, hasLine: boolean): boolean {
  if (path.startsWith("/")) return path.length > 1;
  if (path.startsWith("~/")) return path.length > 2;
  if (!/[\p{L}\p{N}_]/u.test(path)) return false;
  return path.includes("/") || (hasLine && /[^.]\.[^.]/.test(path));
}

/** A path found in the terminal's rows, where it is on screen. */
export interface PathMatch {
  path: string;
  line?: number;
  /** The cells it fills, one segment per row. */
  segments: LinkSegment[];
  /** The ranges to offer it over as xterm's link ranges, 1-based, the end inclusive (see
   * `matchCells`). */
  ranges: IBufferRange[];
  /** It runs over a break the program made in the line, so its pieces are candidates as well. */
  joined?: boolean;
}

const PATH_CHAR_CELL = new RegExp(`^${PATH_CHAR}$`);

/** The narrowest width taken as the one a program laid its lines out at. A program's text width is
 * never this narrow, so a block of lines that are all shorter is a listing (`src/` / `lib/`), not
 * prose wrapped at its longest line, and only a line that fills the terminal counts as a wrapped one. */
const MIN_LAYOUT_WIDTH = 40;

/**
 * Whether `row` may carry on a path that ends on the line `above`, and the column it does from.
 * Claude Code breaks a long line itself, in the middle of a path and at its own width, and
 * indents the next row. That is the case when the line above reached the width it was laid out
 * at and ends in a path character, and the next row starts with one. A line that fills the terminal
 * did; so did the longest line of its block when output was laid out narrower than the terminal
 * shows it (but see `MIN_LAYOUT_WIDTH`). It is only a candidate: the daemon says whether the joined
 * text names anything, and the pieces stay candidates too.
 */
function continuesPath(above: LineRow[], row: TerminalRow, layout: LineLayout): number | undefined {
  const length = lineLength(above);
  if (length < layout.cols && !(length >= MIN_LAYOUT_WIDTH && length >= layout.longest)) return undefined;
  const cells = lineCells(above);
  const end = textEnd(cells);
  if (end === 0 || !PATH_CHAR_CELL.test(cells[end - 1])) return undefined;
  const from = indentOf(row.cells);
  return PATH_CHAR_CELL.test(row.cells[from] ?? "") ? from : undefined;
}

const PATH_CHAIN: LineChain = { continues: continuesPath, maxChars: MAX_PATH_CHARS };

/** The candidates in a joined line that run over a program break. Each break is also taken as a
 * start of its own, since the row above may end in a word that is not part of the path
 * (`looks right:` / `/private/…`). */
function joinedCandidates(text: string, joins: number[]): PathCandidate[] {
  if (joins.length === 0) return [];
  const seen = new Set<string>();
  const found: PathCandidate[] = [];
  for (const offset of [0, ...joins]) {
    for (const candidate of pathCandidates(text.slice(offset))) {
      const start = candidate.start + offset;
      const end = candidate.end + offset;
      if (!crossesJoin(joins, start, end) || seen.has(`${start}:${end}`)) continue;
      seen.add(`${start}:${end}`);
      found.push({ ...candidate, start, end });
    }
  }
  return found;
}

/** The matches of `candidates`, which are in the text of a line. */
function matchesIn({ at, joins, apart }: LineText, candidates: PathCandidate[]): PathMatch[] {
  return candidates.map(({ start, end, path, line }) => {
    const joined = crossesJoin(joins, start, end);
    return {
      path,
      ...(line !== undefined && { line }),
      ...matchCells(at, start, end, apart),
      ...(joined && { joined }),
    };
  });
}

/** The paths in the logical lines touching rows `from` to `to` (1-based buffer lines, both
 * inclusive), a path soft-wrapped by xterm onto the next row included. A path the program itself
 * broke onto another row is found twice over: joined, and as the pieces on each row. Which of the
 * two is the path is for the daemon to say (see `foundPathMatches`). */
export function pathMatchesIn(
  rowAt: (y: number) => TerminalRow | undefined,
  from: number,
  to: number,
): PathMatch[] {
  const read = cachedRows(rowAt);
  const matches: PathMatch[] = [];
  for (const line of logicalLinesIn(read, from, to)) {
    const content = lineText(line);
    matches.push(...matchesIn(content, pathCandidates(content.text)));
  }
  for (const line of logicalLinesIn(read, from, to, PATH_CHAIN)) {
    const content = lineText(line);
    matches.push(...matchesIn(content, joinedCandidates(content.text, content.joins)));
  }
  return matches;
}

/** Whether every cell of `piece` is one of `whole`'s. A match's range would not do: in a table, the
 * cells of other columns lie between a joined match's ends. */
function covers(whole: LinkSegment[], piece: LinkSegment[]): boolean {
  return piece.every(({ y, col, width }) =>
    whole.some((segment) => segment.y === y && segment.col <= col && col + width <= segment.col + segment.width),
  );
}

/** The matches `found` says are paths. A joined match wins over the pieces it is made of, which
 * count only where no joined match was found over them. */
export function foundPathMatches(matches: PathMatch[], found: (path: string) => boolean): PathMatch[] {
  const hits = matches.filter((match) => found(match.path));
  const joined = hits.filter((match) => match.joined);
  return hits.filter((match) => match.joined || !joined.some(({ segments }) => covers(segments, match.segments)));
}

/**
 * What the daemon has said the terminal's candidate paths name. Candidates are asked about once,
 * gathered for a moment into one batch, with at most two requests out at a time; a path that has
 * left the screen by then is not asked about. It is asked again only when its answer is old. Until
 * an answer arrives, or if the path is not one, it is no link. `onChange` runs when an answer makes
 * a path a link or stops it being one. The cache belongs to one session's project, so it is cleared
 * when the terminal shows another.
 */
export class PathLinkCache {
  private readonly entries = new Map<string, PathAnswer & { at: number }>();
  private readonly asking = new Set<string>();
  private queued: string[] = [];
  private visible?: Set<string>;
  private timer?: ReturnType<typeof setTimeout>;
  private inFlight = 0;
  private generation = 0;

  constructor(
    private readonly probe: (paths: string[]) => Promise<PathAnswer[]>,
    private readonly onChange: () => void,
    private readonly now: () => number = Date.now,
    private readonly delay = PROBE_DELAY_MS,
  ) {}

  /** The kind of a path the daemon found, or `undefined` for one it did not or has not answered for. */
  kind(path: string): Exclude<PathLinkKind, "none"> | undefined {
    const kind = this.entries.get(path)?.kind;
    return kind === "file" || kind === "directory" ? kind : undefined;
  }

  /** What the daemon answered for a path, as far as a click needs it. */
  answerFor(path: string): Pick<PathLinkTarget, "kind" | "view" | "viewerOnly"> {
    const { view, viewerOnly } = this.entries.get(path) ?? {};
    return { kind: this.kind(path), view, viewerOnly };
  }

  /** Records which paths are on screen now, so that a queued one that has left it is dropped. */
  setVisible(paths: Iterable<string>): void {
    this.visible = new Set(paths);
  }

  /** Asks about the paths that have no recent answer and are not being asked about. */
  want(paths: Iterable<string>): void {
    const now = this.now();
    for (const path of paths) {
      const entry = this.entries.get(path);
      const ttl = entry?.kind === "none" ? NONE_TTL_MS : FOUND_TTL_MS;
      if ((entry && now - entry.at < ttl) || this.asking.has(path)) continue;
      this.asking.add(path);
      this.queued.push(path);
    }
    this.schedule();
  }

  /** Forgets every answer and drops the ones still on their way. */
  clear(): void {
    this.generation++;
    clearTimeout(this.timer);
    this.timer = undefined;
    this.inFlight = 0;
    this.entries.clear();
    this.asking.clear();
    this.queued = [];
    this.visible = undefined;
  }

  private schedule(): void {
    if (this.timer !== undefined || this.queued.length === 0 || this.inFlight >= MAX_PROBES_IN_FLIGHT) return;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      this.flush();
    }, this.delay);
  }

  private flush(): void {
    const { visible } = this;
    const wanted = visible ? this.queued.filter((path) => visible.has(path)) : this.queued;
    const kept = new Set(wanted);
    for (const path of this.queued) if (!kept.has(path)) this.asking.delete(path);
    this.queued = wanted;
    while (this.queued.length > 0 && this.inFlight < MAX_PROBES_IN_FLIGHT) {
      const batch = this.queued.splice(0, MAX_PROBED_PATHS);
      const generation = this.generation;
      this.inFlight++;
      this.probe(batch).then(
        (answers) => this.answer(generation, batch, answers),
        () => this.answer(generation, batch, undefined),
      );
    }
    this.schedule();
  }

  /** `answers` is `undefined` for a probe that failed: what was known of a path then stays, as of now,
   * and a path not known at all is taken not to be one. */
  private answer(generation: number, batch: string[], answers: PathAnswer[] | undefined): void {
    if (generation !== this.generation) return;
    this.inFlight--;
    if (this.entries.size > MAX_ENTRIES) {
      const oldest = [...this.entries.entries()].sort((a, b) => a[1].at - b[1].at);
      for (const [path] of oldest.slice(0, MAX_ENTRIES / 2)) this.entries.delete(path);
    }
    let changed = false;
    const at = this.now();
    batch.forEach((path, i) => {
      this.asking.delete(path);
      const before = this.kind(path);
      const known = this.entries.get(path);
      const answer: PathAnswer = answers ? (answers[i] ?? { kind: "none" }) : (known ?? { kind: "none" });
      this.entries.set(path, { ...answer, at });
      if (before !== this.kind(path)) changed = true;
    });
    if (changed) this.onChange();
    this.schedule();
  }
}

/** The cells of the paths on rows `from` to `to` that the daemon found, to colour as links. It also
 * notes which paths are on screen, and asks about the ones it has no answer for. */
function pathSegmentsIn(
  cache: PathLinkCache,
  read: (y: number) => TerminalRow | undefined,
  from: number,
  to: number,
): LinkSegment[] {
  const matches = pathMatchesIn(read, from, to);
  cache.setVisible(matches.map((match) => match.path));
  cache.want(matches.map((match) => match.path));
  return mergeSegments(
    foundPathMatches(matches, (path) => !!cache.kind(path))
      .flatMap((match) => match.segments)
      .filter((segment) => segment.y >= from && segment.y <= to),
  );
}

/**
 * The link provider for paths the daemon found. A path it has no answer for yet is no link, but
 * is asked about, so the pointer meeting it again a moment later finds one.
 */
function pathLinkProvider(
  term: Terminal,
  cache: PathLinkCache,
  open: (target: PathLinkTarget, onHost: boolean) => Promise<void>,
  hasSelection: () => boolean,
): ILinkProvider {
  const rowAt = terminalRowReader(term);
  return {
    provideLinks(y, callback) {
      const matches = pathMatchesIn(rowAt, y, y).filter((match) => match.segments.some((segment) => segment.y === y));
      cache.want(matches.map((match) => match.path));
      const links: ILink[] = foundPathMatches(matches, (path) => !!cache.kind(path))
        .flatMap((match) => {
          const target: PathLinkTarget = { path: match.path, line: match.line, ...cache.answerFor(match.path) };
          const activate = terminalLinkHandler((_, event) => open(target, isPrimaryModifier(event)), hasSelection);
          return match.ranges
            .filter((range) => range.start.y <= y && y <= range.end.y)
            .map((range) => ({ range, text: match.path, activate }));
        });
      callback(links.length > 0 ? links : undefined);
    },
  };
}

/**
 * Makes the file paths in `term`'s output links once the daemon says they are files or folders:
 * a link provider for the pointer, registered after the address providers (an address is never a
 * candidate, so the two never meet), and the cells to colour, for `TerminalLinkColors`. `onChange`
 * runs when an answer changes what is a link, for the colours to be drawn again.
 */
export function installPathLinks(
  term: Terminal,
  handlers: {
    probe: (paths: string[]) => Promise<PathAnswer[]>;
    open: (target: PathLinkTarget, onHost: boolean) => Promise<void>;
  },
  hasSelection: () => boolean,
  onChange: () => void,
): { cache: PathLinkCache; segments: ExtraLinkSegments } {
  const cache = new PathLinkCache(handlers.probe, onChange);
  term.registerLinkProvider(pathLinkProvider(term, cache, handlers.open, hasSelection));
  return { cache, segments: (read, from, to) => pathSegmentsIn(cache, read, from, to) };
}
