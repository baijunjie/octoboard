import { useState } from "react";

import { createPersistedPreference } from "../persistedPreference";
import { PREFERENCE_KEYS } from "../preferenceKeys";
import { bindingsEqual, matches, parseBinding, type Binding } from "./binding";
import { defaultBinding, isCommandId, REBINDABLE_COMMANDS, type CommandId } from "./commands";
import { validateBinding, type ShortcutProblem } from "./validate";

/** The bindings that differ from the defaults, by command id; a command left out has its default. */
export type Overrides = Partial<Record<CommandId, Binding>>;

/** Every rebindable command's binding, the defaults with `overrides` on top. */
export function effectiveBindings(overrides: Overrides): Record<CommandId, Binding> {
  return Object.fromEntries(REBINDABLE_COMMANDS.map((command) => [command.id, overrides[command.id] ?? command.binding])) as Record<CommandId, Binding>;
}

/**
 * The overrides a stored or imported value holds, with what cannot stand dropped: an id that names
 * no command, a binding that is not one, one equal to the default, and one that fails
 * `validateBinding` against the bindings the whole set leaves, so a swap or a chain of moves stands
 * whatever order it is written in. Two entries that claim one combination leave the earlier one in
 * the value's own order. `skipped` counts the entries dropped for any reason but being the default
 * already.
 */
export function sanitizeOverrides(value: unknown): { overrides: Overrides; skipped: number } {
  const overrides: Overrides = {};
  let skipped = 0;
  if (typeof value !== "object" || value === null || Array.isArray(value)) return { overrides, skipped };
  const record = value as Record<string, unknown>;
  const candidates: [CommandId, Binding][] = [];
  for (const id of Object.keys(record)) {
    const binding = parseBinding(record[id]);
    if (!isCommandId(id) || !binding) skipped++;
    else if (!bindingsEqual(binding, defaultBinding(id))) candidates.push([id, binding]);
  }
  // Dropping an entry gives its command its default back, which can clash with another entry, so
  // the set is judged again after each drop until none is left to drop.
  for (;;) {
    const effective = effectiveBindings(Object.fromEntries(candidates));
    const drop = candidates.findIndex(([id, binding], index) => {
      const problem = validateBinding(id, binding, effective);
      if (!problem) return false;
      if (problem.reason !== "conflict" || !isCommandId(problem.with)) return true;
      const holder = candidates.findIndex(([other]) => other === problem.with);
      return holder === -1 || holder < index;
    });
    if (drop === -1) break;
    candidates.splice(drop, 1);
    skipped++;
  }
  return { overrides: Object.fromEntries(candidates), skipped };
}

/** The user's shortcut overrides, kept in this client's `localStorage`. */
export const shortcutOverrides = createPersistedPreference<Overrides>(
  PREFERENCE_KEYS.keyboardShortcuts,
  (raw) => {
    if (raw === null) return {};
    try {
      return sanitizeOverrides(JSON.parse(raw)).overrides;
    } catch {
      return {};
    }
  },
  (value) => (Object.keys(value).length === 0 ? null : JSON.stringify(value)),
);

export function bindingFor(id: CommandId): Binding {
  return shortcutOverrides.get()[id] ?? defaultBinding(id);
}

/** The command's binding, re-rendering the caller when it changes. */
export function useBinding(id: CommandId): Binding {
  return shortcutOverrides.useValue()[id] ?? defaultBinding(id);
}

/** The command's binding like `useBinding`, but the same object for as long as the keys are the same, for a caller that must not react to a binding set again to what it was. */
export function useStableBinding(id: CommandId): Binding {
  const binding = useBinding(id);
  const [stable, setStable] = useState(binding);
  if (!bindingsEqual(stable, binding)) setStable(binding);
  return stable;
}

/** Every command's binding, re-rendering the caller when any changes. */
export function useBindings(): Record<CommandId, Binding> {
  return effectiveBindings(shortcutOverrides.useValue());
}

/** Whether `event` is the command's key combination as it is bound now. */
export function matchesCommand(id: CommandId, event: KeyboardEvent): boolean {
  return matches(bindingFor(id), event);
}

function removeOverride(id: CommandId): void {
  const { [id]: _override, ...rest } = shortcutOverrides.get();
  shortcutOverrides.set(rest);
}

/** Puts the command's default binding back and returns `undefined`, or returns why it cannot be (another command has taken that combination) and changes nothing. */
export function restoreBinding(id: CommandId): ShortcutProblem | undefined {
  const problem = validateBinding(id, defaultBinding(id), effectiveBindings(shortcutOverrides.get()));
  if (problem) return problem;
  removeOverride(id);
  return undefined;
}

export function restoreAllBindings(): void {
  shortcutOverrides.set({});
}

/** Binds `id` to `binding` and returns `undefined`, or returns why it cannot be and changes nothing. */
export function setBinding(id: CommandId, binding: Binding): ShortcutProblem | undefined {
  const problem = validateBinding(id, binding, effectiveBindings(shortcutOverrides.get()));
  if (problem) return problem;
  if (bindingsEqual(binding, defaultBinding(id))) removeOverride(id);
  else shortcutOverrides.set({ ...shortcutOverrides.get(), [id]: binding });
  return undefined;
}

/** Replaces every override with those the imported `value` holds; says how many stood and how many entries could not. */
export function replaceOverrides(value: unknown): { applied: number; skipped: number } {
  const { overrides, skipped } = sanitizeOverrides(value);
  shortcutOverrides.set(overrides);
  return { applied: Object.keys(overrides).length, skipped };
}

/** Whether two sets of overrides bind every command alike. */
export function overridesEqual(a: Overrides, b: Overrides): boolean {
  return REBINDABLE_COMMANDS.every((command) => bindingsEqual(a[command.id] ?? command.binding, b[command.id] ?? command.binding));
}
