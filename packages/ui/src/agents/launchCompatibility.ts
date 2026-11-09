import type { Agent, ProviderInfo, SettingDescriptor } from "../protocol";
import { servedProvider } from "./providers";

/** Explicit compatibility values stored on a launch; missing switches follow its parent. */
export type Compatibility = Record<string, boolean>;
/** Edits to compatibility values, with `null` clearing an explicit value. */
export type CompatibilityChanges = Record<string, boolean | null>;

/** Compatibility switches in the provider's declared order. */
export function compatibilitySettings(providers: readonly ProviderInfo[], agent: Agent): SettingDescriptor[] {
  return servedProvider(providers, agent)?.settings.filter((setting) => setting.compatibility_with !== null) ?? [];
}

/** One switch set or cleared, leaving the other explicit values intact. */
export function withCompatibility(current: Compatibility, setting: string, value: boolean | null): Compatibility {
  const next = { ...current };
  if (value === null) delete next[setting];
  else next[setting] = value;
  return next;
}

/** Only changed switches, with removed values represented as clears. */
export function changedCompatibility(current: Compatibility, initial: Compatibility): CompatibilityChanges {
  return Object.fromEntries([...new Set([...Object.keys(current), ...Object.keys(initial)])]
    .filter((setting) => current[setting] !== initial[setting])
    .map((setting) => [setting, current[setting] ?? null]));
}

/** Apply explicit edits, clearing switches whose edits are `null`. */
export function applyCompatibility(current: Compatibility, changes: CompatibilityChanges | undefined): Compatibility {
  let next = current;
  for (const [setting, value] of Object.entries(changes ?? {})) next = withCompatibility(next, setting, value);
  return next;
}
