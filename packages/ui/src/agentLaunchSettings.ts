import type { Translate } from "./i18n/catalog";
import type { AgentModel } from "./protocol";

// How each agent takes a model and an effort, and what it does with one it cannot honour, is in
// the "Model and reasoning effort" note of each agent's section ("Claude Code", "Codex",
// "Grok Build") in docs/agent-cli-reference.md. What the console and project dialogs offer is not
// kept here: the daemon lists it when the dialog asks (`list_agent_models`), so it cannot go stale
// in this source.

/** One entry of a launch-setting picker. */
export type LaunchSettingOption = { value: string; label: string; badge?: string };

/** The reasoning effort levels the agent's models accept, for the effort picker. A chosen model that
 * is listed gets exactly its own, and `[]` (it takes none) is `none`. Otherwise — nothing chosen or
 * a model the list does not name — the levels of every listed model, in the order they were
 * listed, each once. */
export function effortLevels(models: AgentModel[], model: string | null): { levels: string[]; none: boolean } {
  const chosen = models.find((candidate) => candidate.id === model)?.efforts;
  if (chosen) return { levels: chosen, none: chosen.length === 0 };
  return { levels: [...new Set(models.flatMap((candidate) => candidate.efforts))], none: false };
}

/** The options one of the launch-setting pickers offers: the agent's own list, with a stored
 * value that is no longer in it kept on the end, so the dialog never silently changes what it is set
 * to, and marked as not in the list once `listed` says the list is known: while it is still out, or
 * when it could not be read, nothing is known of the value's absence. Unset is the picker's empty
 * state, not an option. */
export function launchSettingOptions(
  t: Translate,
  known: LaunchSettingOption[],
  current: string | null,
  listed: boolean,
): LaunchSettingOption[] {
  if (!current || known.some((option) => option.value === current)) return known;
  return [...known, { value: current, label: current, ...(listed && { badge: t("dialog.agentSettings.unlisted") }) }];
}
