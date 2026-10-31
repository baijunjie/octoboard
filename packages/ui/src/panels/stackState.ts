import type { AsideOwner } from "../layout/asideOwner";

/** The kinds of panel the right pane can stack, in the order they are stacked top to bottom and the
 * order F6 visits them. A later kind (a browser) is one more entry here and one in
 * `panelRegistry.tsx`. */
export const PANEL_KINDS = ["report", "terminal", "files", "processes"] as const;
export type PanelKind = (typeof PANEL_KINDS)[number];

/** Who owns the right pane (`layout/asideOwner.ts`'s kinds): each owner has a stack of its own kinds,
 * in stacking order, which is also the order of their toggles in the top bar. A project's includes
 * the Report panel, which is not the project's but that of the project session selected, and so is
 * kept apart from the stack the project's sessions share (`SESSION_KINDS`, `usePanelStacks.ts`). */
export type OwnerKind = AsideOwner["kind"];
export const OWNER_KINDS: Record<OwnerKind, readonly PanelKind[]> = { project: ["report", "terminal", "files", "processes"], report: ["report", "processes"] };

/** The kinds of an owner's stack that belong to a session rather than to the owner: whether one is
 * open is kept per session, not in the stack every session of the owner shares. */
export const SESSION_KINDS: Record<OwnerKind, readonly PanelKind[]> = { project: ["report"], report: [] };

/** The kinds of `owner` with nothing to show just now: its session kinds, while there is no session
 * to show them for (`reportSessionId` undefined: a project's selected session with no pages). */
export function unavailableKinds(owner: OwnerKind | undefined, reportSessionId: string | undefined): readonly string[] {
  return owner && reportSessionId === undefined ? SESSION_KINDS[owner] : [];
}

/** The kinds open in an owner's stack until the user changes it. The Terminal panel is not among
 * them: opening it starts a shell when the project has none, which only the user's own opening
 * should do. */
const DEFAULT_OPEN: Record<OwnerKind, readonly PanelKind[]> = { project: ["files"], report: ["report"] };

/** The F6 region a kind's panel is: the `data-region` its card carries. */
export type PanelRegion = `panel-${PanelKind}`;

export function panelRegion(kind: PanelKind): PanelRegion {
  return `panel-${kind}`;
}

/** The DOM id of a kind's panel, which its top-bar toggle names in `aria-controls`. */
export function panelId(kind: PanelKind): string {
  return `panel-${kind}`;
}

/** The smallest height a panel in the stack can be resized to: its header and a little of its body. */
export const MIN_PANEL_HEIGHT = 120;

/**
 * What the right pane's stack of panels is, apart from where it is on screen: which kinds are open,
 * each one's share of the stack's height, and which one, if any, is maximized: a maximized panel
 * leaves the pane's column to cover the agent terminal's area, while the others stay in the column
 * as they were (only a kind whose `panelRegistry.tsx` entry is `maximizable` offers it). Kinds are plain
 * strings so a test can stack a kind the app does not have; every function takes the stacking
 * `order` and defaults to the app's. An owner has a stack of its own (`OwnerKind`).
 *
 * Each kind is open at most once. `open` is kept in stacking order and `shares` holds the open
 * kinds' relative height weights. `sharesOf` normalizes the weights within whichever column is
 * being laid out, including a state from hand-edited storage. The pane shows while `open` is not
 * empty: closing the last panel hides it. `maximized` is not persisted.
 */
export interface PanelStack {
  open: readonly string[];
  shares: Readonly<Record<string, number>>;
  maximized?: string;
  /** The leading column's preferred fraction of the width shared by two columns. */
  columnShare?: number;
}

/** What an owner's stack is until the user changes it: its default kinds open (`DEFAULT_OPEN`). */
export function defaultStack(owner: OwnerKind): PanelStack {
  return { open: DEFAULT_OPEN[owner], shares: {} };
}

type Order = readonly string[];

function ordered(kinds: Iterable<string>, order: Order): string[] {
  const set = new Set(kinds);
  return order.filter((kind) => set.has(kind));
}

/** The open kinds, in stacking order. */
export function shownKinds(stack: PanelStack, order: Order = PANEL_KINDS): string[] {
  return ordered(stack.open, order);
}

/** The open kind that is maximized, if one is. */
export function maximizedKind(stack: PanelStack, order: Order = PANEL_KINDS): string | undefined {
  return stack.maximized !== undefined && shownKinds(stack, order).includes(stack.maximized) ? stack.maximized : undefined;
}

/** The open kinds laid out in the pane's column, in stacking order: all but the maximized one. */
export function stackedKinds(stack: PanelStack, order: Order = PANEL_KINDS): string[] {
  const maximized = maximizedKind(stack, order);
  return shownKinds(stack, order).filter((kind) => kind !== maximized);
}

/** The shares of `kinds` as fractions of one: the stored ones scaled to add up to 1, or an even
 * split when one is missing or not a positive number. */
export function sharesOf(stack: PanelStack, kinds: readonly string[]): number[] {
  const stored = kinds.map((kind) => stack.shares[kind]);
  const total = stored.reduce<number>((sum, share) => sum + (share ?? 0), 0);
  if (stored.some((share) => share === undefined || !(share > 0)) || !(total > 0)) return kinds.map(() => 1 / kinds.length);
  return stored.map((share) => share! / total);
}

function withShares(stack: PanelStack, kinds: readonly string[], shares: readonly number[]): PanelStack {
  return { ...stack, open: kinds, shares: Object.fromEntries(kinds.map((kind, index) => [kind, shares[index]])) };
}

/** The stack with `kind` opened. It takes an even share, `1 / n` of `n` panels, from the others in
 * proportion to theirs. A maximized panel stays maximized. */
export function openPanel(stack: PanelStack, kind: string, order: Order = PANEL_KINDS): PanelStack {
  const open = ordered(stack.open, order);
  if (open.includes(kind)) return stack;
  const kinds = ordered([...open, kind], order);
  const share = 1 / kinds.length;
  const others = sharesOf(stack, open).map((existing) => existing * (1 - share));
  const shares = kinds.map((candidate) => (candidate === kind ? share : others[open.indexOf(candidate)]));
  return withShares(stack, kinds, shares);
}

/** The stack with `kind` closed, its share given to the others in proportion to theirs; closing the
 * maximized panel restores the stack. */
export function closePanel(stack: PanelStack, kind: string, order: Order = PANEL_KINDS): PanelStack {
  const open = shownKinds(stack, order);
  if (!open.includes(kind)) return stack;
  const kinds = open.filter((candidate) => candidate !== kind);
  const maximized = stack.maximized === kind ? undefined : stack.maximized;
  if (kinds.length === 0) return { open: [], shares: {}, ...(stack.columnShare === undefined ? {} : { columnShare: stack.columnShare }) };
  const remaining = sharesOf(stack, open).filter((_, index) => open[index] !== kind);
  return withShares({ ...stack, maximized }, kinds, remaining);
}

/** The stack with `kind` maximized, or restored when it already is. */
export function toggleMaximized(stack: PanelStack, kind: string): PanelStack {
  return { ...stack, maximized: stack.maximized === kind ? undefined : kind };
}

/** The stack with no panel maximized: the layout it had before, as maximizing changes nothing else. */
export function restoreStack(stack: PanelStack): PanelStack {
  return stack.maximized === undefined ? stack : { ...stack, maximized: undefined };
}

/** What pressing `kind`'s toggle in the top bar does: it opens the panel when it is not open (which
 * shows the pane when it was hidden), and otherwise closes it, maximized or not; closing the last
 * one hides the pane. */
export function toggleKind(stack: PanelStack, kind: string, order: Order = PANEL_KINDS): PanelStack {
  return shownKinds(stack, order).includes(kind) ? closePanel(stack, kind, order) : openPanel(stack, kind, order);
}

/**
 * Moves the boundary between the panel at `index` and the one below it so the upper one is
 * `upperHeight` px tall, the pair keeping their combined height, neither going under `minHeight`.
 * `available` is the height the stack's panels share. Nothing changes when the pair has no room for
 * two minimums.
 */
export function resizeBoundary(
  stack: PanelStack,
  index: number,
  upperHeight: number,
  available: number,
  minHeight: number = MIN_PANEL_HEIGHT,
  order: Order = PANEL_KINDS,
): PanelStack {
  const kinds = shownKinds(stack, order);
  if (index < 0 || index + 1 >= kinds.length || !(available > 0)) return stack;
  const shares = sharesOf(stack, kinds);
  const pair = shares[index] + shares[index + 1];
  const minShare = minHeight / available;
  if (pair < 2 * minShare) return stack;
  const upper = Math.min(Math.max(upperHeight / available, minShare), pair - minShare);
  return withShares(stack, kinds, shares.map((share, at) => (at === index ? upper : at === index + 1 ? pair - upper : share)));
}

/** Gives the pair either side of the boundary at `index` an even split of their combined height. */
export function resetBoundary(stack: PanelStack, index: number, order: Order = PANEL_KINDS): PanelStack {
  const kinds = shownKinds(stack, order);
  if (index < 0 || index + 1 >= kinds.length) return stack;
  const shares = sharesOf(stack, kinds);
  const half = (shares[index] + shares[index + 1]) / 2;
  return withShares(stack, kinds, shares.map((share, at) => (at === index || at === index + 1 ? half : share)));
}

/** Every owner's stack, as it is kept. */
export type PanelStacks = Record<OwnerKind, PanelStack>;

function parseOne(value: unknown, owner: OwnerKind): PanelStack {
  const stored = OWNER_KINDS[owner].filter((kind) => !SESSION_KINDS[owner].includes(kind));
  if (typeof value !== "object" || value === null) return defaultStack(owner);
  const { open, shares, columnShare } = value as { open?: unknown; shares?: unknown; columnShare?: unknown };
  if (!Array.isArray(open)) return defaultStack(owner);
  const kinds = ordered(open.filter((kind): kind is string => typeof kind === "string"), stored);
  const storedShares = typeof shares === "object" && shares !== null ? (shares as Record<string, unknown>) : {};
  const kept = Object.fromEntries(kinds.flatMap((kind) => (typeof storedShares[kind] === "number" ? [[kind, storedShares[kind]]] : [])));
  return { open: kinds, shares: kept, ...(typeof columnShare === "number" && Number.isFinite(columnShare) && columnShare > 0 && columnShare < 1 ? { columnShare } : {}) };
}

/** Reads the stored text; whatever is not a well-formed stack of known kinds is the default.
 * `legacyHidden` is the answer of the pane's old visibility preference, which a pane the user hid
 * carries over as no panel open, when nothing is stored yet. */
export function parseStacks(raw: string | null, legacyHidden = false): PanelStacks {
  const defaults = (): PanelStacks => ({
    project: legacyHidden ? { open: [], shares: {} } : defaultStack("project"),
    report: legacyHidden ? { open: [], shares: {} } : defaultStack("report"),
  });
  if (raw === null) return defaults();
  try {
    const value: unknown = JSON.parse(raw);
    if (typeof value !== "object" || value === null) return defaults();
    const record = value as Record<string, unknown>;
    return { project: parseOne(record.project, "project"), report: parseOne(record.report, "report") };
  } catch {
    return defaults();
  }
}

/** The text to store: each owner's open kinds and their shares, not the maximized one. */
export function serializeStacks(stacks: PanelStacks): string {
  const one = ({ open, shares, columnShare }: PanelStack) => ({ open, shares, columnShare });
  return JSON.stringify({ project: one(stacks.project), report: one(stacks.report) });
}

/** Which of the F6 regions the panels are, in the order F6 visits them: the open ones are shown. A
 * maximized panel comes first, as it lies where the agent terminal does, before the pane. */
export function shownPanelRegions(stack: PanelStack | undefined, owner: OwnerKind | undefined): Record<PanelRegion, boolean> {
  const order = owner ? OWNER_KINDS[owner] : [];
  const open = stack && owner ? shownKinds(stack, order) : [];
  const maximized = stack && owner ? maximizedKind(stack, order) : undefined;
  const kinds = maximized === undefined ? PANEL_KINDS : [maximized as PanelKind, ...PANEL_KINDS.filter((kind) => kind !== maximized)];
  return Object.fromEntries(kinds.map((kind) => [panelRegion(kind), open.includes(kind)])) as Record<PanelRegion, boolean>;
}
