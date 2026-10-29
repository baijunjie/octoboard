import { useSyncExternalStore } from "react";
import { useShallow } from "zustand/react/shallow";

import { gitModeWorktree } from "../browser/browserState";
import { t } from "../i18n/language";
import { errorPresentation } from "../errorPresentation";
import { hostPathRecovery } from "../errorRecovery";
import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import type { ShellTerminal } from "../protocol";
import { seedShellTerminal, useDaemonStore, type Daemon, type State } from "../store";

/** A project's shell terminals, oldest first: the order their tabs are in. */
export function shellTerminalsOf(state: Pick<State, "shellTerminals">, project: string): ShellTerminal[] {
  return Array.from(state.shellTerminals.values())
    .filter((terminal) => terminal.project_id === project)
    .sort((a, b) => a.created_at - b.created_at);
}

/** `shellTerminalsOf` for a component, which renders again only when one of them changes. */
export function useProjectShellTerminals(project: string): ShellTerminal[] {
  return useDaemonStore(useShallow((state) => shellTerminalsOf(state, project)));
}

/** How many projects' active tabs are kept; the ones set longest ago go first. */
const MAX_REMEMBERED = 64;

function parseActive(raw: string | null): Record<string, string> {
  if (raw === null) return {};
  try {
    const value: unknown = JSON.parse(raw);
    if (typeof value !== "object" || value === null || Array.isArray(value)) return {};
    return Object.fromEntries(Object.entries(value).filter((entry): entry is [string, string] => typeof entry[1] === "string"));
  } catch {
    return {};
  }
}

/** The tab each project's Terminal panel shows, by terminal id: a per-client choice, as the open
 * panels are, so another window keeps its own. */
const activePreference = createPersistedPreference<Record<string, string>>(
  PREFERENCE_KEYS.activeShellTerminals,
  parseActive,
  (value) => JSON.stringify(value),
);

/** Makes `terminal` the tab `project`'s Terminal panel shows. */
export function setActiveShellTerminal(project: string, terminal: string): void {
  const current = activePreference.get();
  if (current[project] === terminal) return;
  const kept = Object.entries(current).filter(([key]) => key !== project).slice(-(MAX_REMEMBERED - 1));
  activePreference.set({ ...Object.fromEntries(kept), [project]: terminal });
}

/** The tab `project`'s Terminal panel shows among `terminals`: the one last chosen while it is
 * still there; once it is gone, the one `replacements` names for it (the tab next to one closed
 * from the panel), followed on while that is gone too; otherwise the newest. None while the
 * project has no terminal. */
export function useActiveShellTerminal(project: string, terminals: readonly ShellTerminal[], replacements: ReadonlyMap<string, string> = new Map()): string | undefined {
  const present = (id: string | undefined) => id !== undefined && terminals.some((terminal) => terminal.id === id);
  let candidate: string | undefined = activePreference.useValue()[project];
  const seen = new Set<string>();
  while (candidate !== undefined && !present(candidate) && !seen.has(candidate)) {
    seen.add(candidate);
    candidate = replacements.get(candidate);
  }
  return present(candidate) ? candidate : terminals.at(-1)?.id;
}

/** How many starts are in flight per project, so one start finishing does not end the pending state
 * of another. */
const starting = new Map<string, number>();
/** The starts still pending per project, oldest first, one of which an `ifNone` start joins instead
 * of starting a second shell. A start leaves once it settles, whatever order they settle in. */
const inFlight = new Map<string, Set<Promise<string | undefined>>>();
const startingListeners = new Set<() => void>();
// A new set on each change, as `useSyncExternalStore` compares snapshots by identity.
let startingSnapshot: ReadonlySet<string> = new Set();

function setStarting(project: string, on: boolean): void {
  const count = (starting.get(project) ?? 0) + (on ? 1 : -1);
  if (count > 0) starting.set(project, count);
  else starting.delete(project);
  startingSnapshot = new Set(starting.keys());
  startingListeners.forEach((listener) => listener());
}

function subscribeStarting(listener: () => void): () => void {
  startingListeners.add(listener);
  return () => startingListeners.delete(listener);
}

/** Whether a shell terminal is being started for `project`: the panel shows that instead of its
 * empty state, which would offer a second start. */
export function useShellTerminalStarting(project: string): boolean {
  return useSyncExternalStore(subscribeStarting, () => startingSnapshot).has(project);
}

/** Where a shell starts, when it is not in the worktree the project's Git mode shows: the
 * project's own directory, or a worktree's, and a folder below it, as a wire path. */
export interface ShellTerminalPlace {
  worktree?: string;
  path?: string;
}

/** How long a request to focus a started terminal stays good, for a panel that never shows it. */
const FOCUS_REQUEST_MS = 5000;

/** The terminal a start from outside the panel asked to have focused, until the panel shows it. */
let focusRequested: { terminal: string; at: number } | undefined;

/** Whether `terminal` is the one a start asked to focus once shown; asking takes the request,
 * which a terminal that is shown takes whether or not it is acted on. */
export function takeShellTerminalFocus(terminal: string): boolean {
  const request = focusRequested;
  if (request?.terminal !== terminal) return false;
  focusRequested = undefined;
  return Date.now() - request.at < FOCUS_REQUEST_MS;
}

/**
 * Starts a shell terminal for `project` in `place`, or in the worktree its Git mode shows
 * (`gitModeWorktree`) when none is given, the project's own directory when it shows none, and makes
 * it the project's tab once the daemon has it. `focus` asks the panel to focus the terminal once it
 * shows it, for a start made from outside the panel (`takeShellTerminalFocus`). `ifNone` starts one
 * only while the project has none and none is being started, as opening the Terminal panel does;
 * while one is being started it resolves with the result of the earliest start still pending
 * instead of starting another. A refusal is reported as an error. Resolves with the new terminal's
 * id, or nothing when none was started.
 */
export function startShellTerminal(
  daemon: Pick<Daemon, "request" | "store" | "toastError">,
  project: string,
  { ifNone = false, place, focus = false }: { ifNone?: boolean; place?: ShellTerminalPlace; focus?: boolean } = {},
): Promise<string | undefined> {
  if (ifNone) {
    const earliest = inFlight.get(project)?.values().next().value;
    if (earliest) return earliest;
    if (shellTerminalsOf(daemon.store.getState(), project).length > 0) return Promise.resolve(undefined);
  }
  const pending = inFlight.get(project) ?? new Set();
  inFlight.set(project, pending);
  const started: Promise<string | undefined> = createShellTerminal(daemon, project, place, focus).finally(() => {
    pending.delete(started);
    if (pending.size === 0 && inFlight.get(project) === pending) inFlight.delete(project);
  });
  pending.add(started);
  return started;
}

async function createShellTerminal(daemon: Pick<Daemon, "request" | "store" | "toastError">, project: string, place: ShellTerminalPlace | undefined, focus: boolean): Promise<string | undefined> {
  setStarting(project, true);
  try {
    const { worktree, path } = place ?? { worktree: gitModeWorktree(project) };
    const reply = await daemon.request({ type: "create_shell_terminal", project, ...(worktree !== undefined && { worktree }), ...(path !== undefined && { path }) });
    if (reply.type !== "shell_terminal_created") return undefined;
    if (focus) focusRequested = { terminal: reply.shell_terminal.id, at: Date.now() };
    // The broadcast of the record can come after this reply; seeded here (a record the broadcast
    // already brought stands), the pending state never ends on a store without it.
    seedShellTerminal(daemon.store, reply.shell_terminal);
    setActiveShellTerminal(project, reply.shell_terminal.id);
    return reply.shell_terminal.id;
  } catch (err) {
    daemon.toastError(errorPresentation(err), undefined, t("shellTerminal.newFailedTitle"), place?.path ? hostPathRecovery(err) : undefined);
    return undefined;
  } finally {
    setStarting(project, false);
  }
}
