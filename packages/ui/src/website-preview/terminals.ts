import type { Agent } from "../protocol";
import { cells, isWide, type TerminalScript } from "../gallery/fakeTerminal";

/** One line of an edit's diff: a removed or an added line of code. */
export type TerminalDiffLine = { removed: string } | { added: string };

/** One thing an agent did or said. Only `say` is the agent's prose; what a tool prints (a read's
 * line count, `result`) is the same in every language, like the paths and commands. */
export type TerminalStep =
  | { say: string[] }
  | { read: string; lines: number }
  | { edit: string; added: number; removed: number; diff?: TerminalDiffLine[] }
  | { run: string; result: string };

export interface TerminalQuestion {
  header: string;
  text: string;
  options: { label: string; detail?: string }[];
}

/** What one session's terminal shows. The steps in `done` are on screen when the page opens; those
 * in `live` come due one by one, counted from the page load, while the session works, and only then. */
export interface SessionTerminalCopy {
  prompt: string;
  /** The status line's verb, such as "Checking". */
  verb: string;
  done: TerminalStep[];
  live: TerminalStep[];
  /** What a session waiting for the user asks, below its `done` steps. */
  question?: TerminalQuestion;
}

export interface TerminalSession {
  id: string;
  agent: Agent;
  status: "working" | "idle" | "waiting_user";
  cwd: string;
  /** The command of a background task the session has running. */
  background?: string;
  copy: SessionTerminalCopy;
}

export interface TerminalOptions {
  /** When the page loaded, which the elapsed timers count from. */
  startedAt: number;
  /** Whether a working session's screen moves; without it the timer and the spinner stand still. */
  animate: boolean;
}

const ESC = "\x1b[";
const RESET = `${ESC}0m`;
const dim = (text: string) => `${ESC}2m${text}${RESET}`;
const bold = (text: string) => `${ESC}1m${text}${RESET}`;
const colour = (code: string, text: string) => `${ESC}${code}m${text}${RESET}`;
const GREEN = "32";
const RED = "31";

/** `line` cut to at most `cols` cells, with an ellipsis where text was dropped. Escape sequences
 * take no cells. */
function clip(line: string, cols: number): string {
  if (cells(line) <= cols) return line;
  let width = 0;
  let out = "";
  // eslint-disable-next-line no-control-regex -- splits the SGR colour codes off the text.
  for (const part of line.split(/(\x1b\[[0-9;]*m)/)) {
    if (part.startsWith("\x1b")) {
      out += part;
      continue;
    }
    for (const ch of part) {
      const w = isWide(ch) ? 2 : 1;
      if (width + w > cols - 1) return `${out}…${RESET}`;
      out += ch;
      width += w;
    }
  }
  return out;
}

/** `text` followed by blanks up to `width` cells. */
const padded = (text: string, width: number) => text + " ".repeat(Math.max(width - cells(text), 0));

/** A box of `inner` cells between its bars, one row for each of `rows`. */
const framed = (rows: string[], inner: number, cols: number) => [
  dim(`╭${"─".repeat(inner)}╮`),
  ...rows.map((row) => clip(`${dim("│")}${padded(row, inner)}${dim("│")}`, cols)),
  dim(`╰${"─".repeat(inner)}╯`),
];

/** `lines` indented: the first behind `first`, the rest behind `rest` (both the same width). */
const hanging = (lines: string[], first: string, rest: string) =>
  lines.map((line, index) => (index === 0 ? first : rest) + line);

/** The lines of an edit's diff, behind `gutter`. */
const diffRows = (diff: TerminalDiffLine[] = [], gutter = "") =>
  diff.map((line) =>
    "removed" in line ? colour(RED, `${gutter}- ${line.removed}`) : colour(GREEN, `${gutter}+ ${line.added}`),
  );

const count = (n: number, noun: string) => `${n} ${noun}${n === 1 ? "" : "s"}`;

function elapsedText(seconds: number): string {
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${String(seconds % 60).padStart(2, "0")}s`;
}

/** What differs between the agents' looks. The rows `resting` returns, with the status line above
 * them while the session works, are the bottom of the screen, which stays at the same height so a
 * tick can rewrite it in place. */
interface Look {
  header(cwd: string, cols: number): string[];
  user(prompt: string): string[];
  step(step: TerminalStep): string[];
  /** The status line of a working session, which sits above its input. */
  status(cols: number, frame: number, verb: string, elapsed: string): string;
  /** The input and the footer of a session at rest. */
  resting(cols: number, background?: string): string[];
  /** The question of a session waiting for the user, in place of its input. */
  asking(cols: number, question: TerminalQuestion): string[];
}

const CLAUDE_ACCENT = "38;5;209";
const CLAUDE_SPINNER = ["·", "✢", "✳", "✶", "✻", "✽"];

const claude: Look = {
  header: (cwd) => [
    colour(`1;${CLAUDE_ACCENT}`, " ✻ Welcome to Claude Code"),
    "",
    dim(`  cwd: ${cwd}`),
  ],
  user: (prompt) => [`${ESC}48;5;237m> ${prompt} ${RESET}`],
  step(step) {
    if ("say" in step) return hanging(step.say, `${colour(GREEN, "●")} `, "  ");
    if ("read" in step) return [`${colour(GREEN, "●")} ${bold("Read")}(${step.read})`, `  ${dim(`⎿  Read ${step.lines} lines`)}`];
    if ("edit" in step) {
      return [
        `${colour(GREEN, "●")} ${bold("Update")}(${step.edit})`,
        `  ${dim("⎿  Updated")} ${dim(`with ${count(step.added, "addition")} and ${count(step.removed, "removal")}`)}`,
        ...diffRows(step.diff).map((row) => `     ${row}`),
      ];
    }
    return [`${colour(GREEN, "●")} ${bold("Bash")}(${step.run})`, `  ${dim(`⎿  ${step.result}`)}`];
  },
  status: (cols, frame, verb, elapsed) =>
    clip(`${colour(CLAUDE_ACCENT, CLAUDE_SPINNER[frame % CLAUDE_SPINNER.length])} ${colour(CLAUDE_ACCENT, `${verb}…`)} ${dim(`(${elapsed} · esc to interrupt)`)}`, cols),
  resting: (cols, background) => [
    "",
    ...framed([` ${bold(">")} `], Math.max(cols - 2, 1), cols),
    clip(dim(background ? `  ? for shortcuts · 1 background task: ${background}` : "  ? for shortcuts"), cols),
  ],
  asking: (cols, question) => [
    "",
    dim("─".repeat(cols)),
    ` ${colour(CLAUDE_ACCENT, "☐")} ${bold(question.header)}`,
    "",
    ` ${bold(question.text)}`,
    "",
    ...question.options.flatMap((option, index) => [
      index === 0
        ? ` ${colour("36", "❯")} ${colour("36", `${index + 1}. ${option.label}`)}`
        : `   ${index + 1}. ${option.label}`,
      ...(option.detail ? [`      ${dim(option.detail)}`] : []),
    ]),
    "",
    dim(" Enter to select · ↑/↓ to navigate · Esc to cancel"),
  ],
};

const codex: Look = {
  header: (cwd, cols) => {
    const inner = Math.max(Math.min(cols - 2, 54), 1);
    return framed([` ${bold(">_ OpenAI Codex")}`, "", ` ${dim(`directory: ${cwd}`)}`], inner, cols);
  },
  user: (prompt) => [`${bold("›")} ${prompt}`],
  step(step) {
    if ("say" in step) return hanging(step.say, "• ", "  ");
    if ("read" in step) return [`• ${bold("Explored")}`, `  ${dim("└")} Read ${step.read} ${dim(`(${step.lines} lines)`)}`];
    if ("edit" in step) {
      return [
        `• ${bold("Edited")} ${step.edit} (${colour(GREEN, `+${step.added}`)} ${colour(RED, `-${step.removed}`)})`,
        ...diffRows(step.diff).map((row) => `    ${row}`),
      ];
    }
    return [`• ${bold("Ran")} ${step.run}`, `  ${dim("└")} ${dim(step.result)}`];
  },
  status: (cols, frame, verb, elapsed) =>
    clip(`${(frame >> 2) & 1 ? "◦" : "•"} ${bold(verb)} ${dim(`(${elapsed} • esc to interrupt)`)}`, cols),
  resting: (cols, background) => [
    "",
    clip(`${bold("›")} ${dim("Ask Codex to do anything")}`, cols),
    "",
    clip(dim(background ? `  ⏎ send   ⌃J newline   ⌃T transcript   ${background} running` : "  ⏎ send   ⌃J newline   ⌃T transcript   ⌃C quit"), cols),
  ],
  asking: (_cols, question) => [
    "",
    `  ${bold(question.text)}`,
    "",
    ...question.options.map((option, index) => `  ${index === 0 ? colour("36", "›") : " "} ${index + 1}. ${option.label}${option.detail ? dim(` — ${option.detail}`) : ""}`),
    "",
    dim("  ↑/↓ to select   ⏎ to confirm   esc to cancel"),
  ],
};

const GROK_ACCENT = "38;5;141";
const GROK_SPINNER = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

function grokInput(cols: number, hint: string): string[] {
  const rule = dim("─".repeat(cols));
  return [rule, clip(`${colour(GROK_ACCENT, "❯")} ${hint}`, cols), rule];
}

const grok: Look = {
  header: (cwd) => [
    colour(`1;${GROK_ACCENT}`, " ◆ Grok Build"),
    dim(`   ${cwd}`),
  ],
  user: (prompt) => [`${colour(GROK_ACCENT, "▍")} ${bold(prompt)}`],
  step(step) {
    if ("say" in step) return hanging(step.say, `${colour(GROK_ACCENT, "◆")} `, "  ");
    if ("read" in step) return [`${colour(GROK_ACCENT, "▸")} Read ${step.read}`, `  ${dim(`└─ Read ${step.lines} lines`)}`];
    if ("edit" in step) {
      return [
        `${colour(GROK_ACCENT, "▸")} Edit ${step.edit} ${colour(GREEN, `+${step.added}`)} ${colour(RED, `−${step.removed}`)}`,
        ...diffRows(step.diff, "│ ").map((row) => `  ${row}`),
      ];
    }
    return [`${colour(GROK_ACCENT, "▸")} Run ${step.run}`, `  ${dim(`└─ ${step.result}`)}`];
  },
  status: (cols, frame, verb, elapsed) =>
    clip(`${colour(GROK_ACCENT, GROK_SPINNER[frame % GROK_SPINNER.length])} ${verb}… ${dim(`${elapsed} · esc to cancel`)}`, cols),
  resting: (cols, background) => [
    "",
    ...grokInput(cols, dim("Message Grok")),
    clip(dim(background ? `  / commands · @ files · ${background} running` : "  / commands · @ files"), cols),
  ],
  asking: (cols, question) => [
    "",
    ` ${colour(GROK_ACCENT, "◆")} ${bold(question.text)}`,
    "",
    ...question.options.map((option, index) => `   ${index === 0 ? colour(GROK_ACCENT, "❯") : " "} ${option.label}${option.detail ? dim(` — ${option.detail}`) : ""}`),
    "",
    ...grokInput(cols, dim("Choose an option")),
  ],
};

const LOOKS: Record<Agent, Look> = { claude, codex, grok };

/** A small number that stays the same for a session id, so each session paces itself differently. */
function seedOf(id: string): number {
  return [...id].reduce((hash, ch) => (hash * 31 + ch.charCodeAt(0)) >>> 0, 7);
}

/** When each live step comes due, in ms after the page load: 4 to 7 seconds apart, differently per
 * session. */
export function liveSchedule(id: string, count: number): number[] {
  const seed = seedOf(id);
  let at = 0;
  return Array.from({ length: count }, (_, index) => {
    at += (4 + ((seed >>> (index * 2)) % 4)) * 1000;
    return at;
  });
}

/** The elapsed time a working session opens on, between 1m 20s and 4m 40s. */
const baseSeconds = (id: string) => 80 + (seedOf(id) % 200);

/** A script that simulates the session's agent in the state its status says: a working session
 * keeps a status line spinning and appends its live steps as they come due, while the others stay
 * as they are. Progress follows the page's clock, not the socket's, so a session that is closed and
 * opened again shows the steps it had reached rather than starting over. */
export function createTerminalScript(session: TerminalSession, options: TerminalOptions): TerminalScript {
  const look = LOOKS[session.agent];
  const { copy } = session;
  const working = session.status === "working";
  const moving = working && options.animate;
  const schedule = moving ? liveSchedule(session.id, copy.live.length) : [];
  const base = baseSeconds(session.id);
  /** How many live steps this script has written to its terminal. */
  let written = 0;

  const dueCount = () => schedule.filter((at) => at <= Date.now() - options.startedAt).length;
  const blocks = (steps: TerminalStep[]) => steps.flatMap((step) => ["", ...look.step(step)]);
  const tail = (cols: number, frame: number): string[] => {
    if (session.status === "waiting_user" && copy.question) return look.asking(cols, copy.question);
    if (!working) return look.resting(cols, session.background);
    const seconds = base + (moving ? Math.floor((Date.now() - options.startedAt) / 1000) : 0);
    const status = look.status(cols, moving ? frame : 0, copy.verb, elapsedText(seconds));
    // The blank row first in `resting` sits above the status line.
    return ["", status, ...look.resting(cols)];
  };

  return {
    screen(cols, step) {
      written = dueCount();
      return [
        ...look.header(session.cwd, cols),
        "",
        ...look.user(copy.prompt),
        ...blocks([...copy.done, ...copy.live.slice(0, written)]),
        ...tail(cols, step),
      ].join("\r\n");
    },
    ...(moving
      ? {
        // The tail is rewritten in place: up to its first row, clear from there, then the steps that
        // have come due since this script last wrote and the tail again. Nothing above it is
        // touched, so a visitor who scrolled up keeps their place.
        tick: (cols: number, step: number) => {
          const rows = tail(cols, step);
          const due = dueCount();
          const added = copy.live.slice(written, due);
          written = due;
          // The tail starts with a blank row, which separates the new steps from it.
          return `${ESC}${rows.length - 1}A\r${ESC}J${[...blocks(added), ...rows].join("\r\n")}`;
        },
      }
      : {}),
  };
}
