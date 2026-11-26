import { TERMINAL_FONT_FAMILY, XTERM_SCROLLBAR_WIDTH } from "../TerminalController";

/** The padding of a shell terminal's view around the element xterm mounts in, which the start size is measured inside. */
export const SHELL_TERMINAL_PADDING = "ps-(--pane-inset) pe-2 py-2";

/** The size of the code text in the other panels in pixels: Tailwind's `text-xs`, which the file viewer's code
 * (`CodeSurface`) and fenced code blocks are drawn at. A shell terminal is drawn at it rather than at the session
 * terminal's font size setting. */
export function shellTerminalFontSize(): number {
  const fallback = 12;
  if (typeof document === "undefined") return fallback;
  const root = document.documentElement;
  const value = getComputedStyle(root).getPropertyValue("--text-xs").trim();
  const match = /^(\d+(?:\.\d+)?)(rem|px)$/.exec(value);
  if (!match) return fallback;
  const size = Number(match[1]);
  if (match[2] === "px") return size;
  const rootSize = parseFloat(getComputedStyle(root).fontSize);
  return Number.isFinite(rootSize) && rootSize > 0 ? size * rootSize : fallback;
}

/** The body of each project's Terminal panel that is on screen, where its views are laid out. */
const bodies = new Map<string, HTMLElement>();
/** Starts waiting for a project's body to appear, each told once it does. */
const waiting = new Map<string, Set<() => void>>();

/** Records `element` as the body of the project's Terminal panel, until the returned function is called. */
export function registerShellTerminalBody(project: string, element: HTMLElement): () => void {
  bodies.set(project, element);
  for (const resolve of waiting.get(project) ?? []) resolve();
  waiting.delete(project);
  return () => {
    if (bodies.get(project) === element) bodies.delete(project);
  };
}

/** The grid a view laid out in `body` is fitted to, as xterm's fit computes it from the space inside the view's
 * padding and the size of a character cell, or nothing while the body has no size. It is an estimate: xterm measures
 * a character on a canvas, rounds a cell to device pixels and truncates the width it fits into, each of which can
 * leave it one column short of what is measured here, so the width is truncated as xterm truncates it and one column
 * is kept back. Fewer columns than the view gets do no harm, as the view's first resize widens the shell; more would
 * wrap the end of its first prompt's mark (zsh's `PROMPT_SP`) onto a line of its own. */
function measureGrid(body: HTMLElement): { cols: number; rows: number } | undefined {
  if (body.clientWidth === 0 || body.clientHeight === 0) return undefined;
  const view = document.createElement("div");
  view.className = `pointer-events-none invisible absolute inset-0 ${SHELL_TERMINAL_PADDING}`;
  view.setAttribute("aria-hidden", "true");
  const space = document.createElement("div");
  space.className = "h-full";
  const cell = document.createElement("span");
  cell.style.cssText = `position:absolute;white-space:pre;font-family:${TERMINAL_FONT_FAMILY};font-size:${shellTerminalFontSize()}px;line-height:normal`;
  cell.textContent = "W".repeat(32);
  space.append(cell);
  view.append(space);
  body.append(view);
  try {
    const { width, height } = cell.getBoundingClientRect();
    const cellWidth = width / 32;
    const cellHeight = Math.ceil(height);
    if (cellWidth <= 0 || cellHeight <= 0) return undefined;
    const room = space.getBoundingClientRect();
    const cols = Math.floor((Math.floor(room.width) - XTERM_SCROLLBAR_WIDTH) / cellWidth) - 1;
    const rows = Math.floor(Math.floor(room.height) / cellHeight);
    return cols >= 2 && rows >= 1 ? { cols, rows } : undefined;
  } finally {
    view.remove();
  }
}

/** Resolves once the next two frames have been drawn, the time a panel opened with the start is given to lay out. */
function twoFrames(): Promise<void> {
  if (typeof requestAnimationFrame !== "function") return new Promise((resolve) => setTimeout(resolve, 32));
  return new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
}

/** The grid a new shell terminal of the project will be shown at, for the shell to start at (`create_shell_terminal`'s
 * `cols` and `rows`): measured in the project's Terminal panel, waiting a little for the panel to appear when the start
 * comes with its opening. Nothing when the panel does not appear by then or has no size, and the daemon's default
 * size stands. */
export async function shellTerminalStartSize(project: string): Promise<{ cols: number; rows: number } | undefined> {
  if (!bodies.has(project)) {
    let resolveShown!: () => void;
    const shown = new Promise<void>((resolve) => (resolveShown = resolve));
    const set = waiting.get(project) ?? new Set();
    waiting.set(project, set);
    set.add(resolveShown);
    await Promise.race([shown, twoFrames()]);
    set.delete(resolveShown);
    if (set.size === 0 && waiting.get(project) === set) waiting.delete(project);
  }
  const body = bodies.get(project);
  return body?.isConnected ? measureGrid(body) : undefined;
}
