import React from "react";

import { AGENT_LABEL } from "../agents/agents";
import { hasClaudeCompatibility } from "../agents/claudeCompatibility";
import { Tag } from "../components/form/Tag";
import { joinNodes } from "../i18n/joinPhrases";
import type { Language } from "../i18n/languages";
import { DELIVERY_CHOICES, DEVELOPMENT_CHOICES, GIT_WORKFLOW_ITEMS, GIT_WORKFLOW_ITEM_KEYS } from "../git/gitWorkflow";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { Message } from "../i18n/react";
import type { Agent, GitWorkflowSettings } from "../protocol";
import type { Binding } from "../shortcuts/binding";
import { REBINDABLE_COMMANDS } from "../shortcuts/commands";
import type { Overrides } from "../shortcuts/store";
import { BindingKbd } from "../shortcuts/ShortcutKbd";
import { sameValue } from "../values/sameValue";
import type { shortcutsReview } from "./importPlan";

/** One value of a composite setting, listed under its line of the import cards: the item and what it is. */
export interface SettingValue {
  item: string;
  value: React.ReactNode;
}

/** What a value reads as in the review: the value alone where `current` is absent (the result, which says what was
 * applied), else with what it is now. */
function shown(value: React.ReactNode, current?: React.ReactNode): React.ReactNode {
  return current === undefined ? value : <Message id="configuration.import.settingValueChange" params={{ value, current }} />;
}

const onOff = (on: boolean): PlainMessageKey => (on ? "configuration.import.value.on" : "configuration.import.value.off");

/** How each item's value is worded in the review, by the item's field (labelled by `GIT_WORKFLOW_ITEMS`). */
const GIT_WORKFLOW_WORDS: Record<keyof GitWorkflowSettings, (workflow: GitWorkflowSettings) => PlainMessageKey> = {
  development: (w) => DEVELOPMENT_CHOICES.find((choice) => choice.value === w.development)!.label,
  squash_before_delivery: (w) => onOff(w.squash_before_delivery),
  rebase_before_delivery: (w) => onOff(w.rebase_before_delivery),
  delivery: (w) => DELIVERY_CHOICES.find((choice) => choice.value === w.delivery)!.label,
  revert_gate: (w) => onOff(w.revert_gate),
};

/** Each git workflow item with its value, named as the settings page names them. With `current`, only the
 * items that differ from it, each with what it is now. */
export function gitWorkflowValues(t: Translate, wanted: GitWorkflowSettings, current?: GitWorkflowSettings): SettingValue[] {
  return GIT_WORKFLOW_ITEM_KEYS
    .filter((field) => !current || wanted[field] !== current[field])
    .map((field) => ({
      item: t(GIT_WORKFLOW_ITEMS[field].label),
      value: shown(t(GIT_WORKFLOW_WORDS[field](wanted)), current && t(GIT_WORKFLOW_WORDS[field](current))),
    }));
}

/** The agents' Claude compatibility, each On or Off. With `current`, only the agents applying would change, each
 * with what it is now: an agent counts only when it has the setting and this machine reports it in `current` (the
 * daemon skips the others, and for an agent it does not report nothing is known to compare with). */
export function claudeCompatibilityValues(t: Translate, wanted: Partial<Record<Agent, boolean>>, current?: Partial<Record<Agent, boolean>>): SettingValue[] {
  return (Object.entries(wanted) as [Agent, boolean][])
    .filter(([agent, on]) => hasClaudeCompatibility(agent) && (!current || (current[agent] !== undefined && current[agent] !== on)))
    .map(([agent, on]) => ({ item: AGENT_LABEL[agent], value: shown(t(onOff(on)), current && t(onOff(current[agent]!))) }));
}

/** An agent's tags as the read-only tags the settings show them as, or the message for no tags. The joining text of
 * the language's list sits between them for a screen reader alone, which would otherwise run them together; the gap
 * between the tags sets them apart on the screen. */
function tagsNode(t: Translate, language: Language, tags: string[]): React.ReactNode {
  if (tags.length === 0) return t("settings.accounts.tags.none");
  return (
    <span className="inline-flex flex-wrap gap-1 align-middle">
      {joinNodes(language, tags, (tag, key) => <Tag key={key} isReadOnly wrap tag={tag} />).map((part, index) => (typeof part === "string" ? <span key={`joint-${index}`} className="sr-only">{part}</span> : part))}
    </span>
  );
}

/** The tags of each agent this build knows. With `current`, only the agents whose tags would change, each
 * with what it has now: the tags of each agent the document lists replace that agent's, an agent it does not list keeps
 * its own, and an agent missing from `current` has no tags. */
export function agentTagsValues(t: Translate, language: Language, wanted: Partial<Record<Agent, string[]>>, current?: Partial<Record<Agent, string[]>>): SettingValue[] {
  return (Object.entries(wanted) as [Agent, string[]][])
    .filter(([agent, tags]) => Object.hasOwn(AGENT_LABEL, agent) && (!current || !sameValue(tags, current[agent] ?? [])))
    .map(([agent, tags]) => ({ item: AGENT_LABEL[agent], value: shown(tagsNode(t, language, tags), current && tagsNode(t, language, current[agent] ?? [])) }));
}

/** A key combination at the size of the muted text it sits in: the key caps' own size is for a row of their own,
 * and would stand out of a line of small text. */
function SmallKbd({ binding }: { binding: Binding }): React.ReactElement {
  return <BindingKbd binding={binding} className="h-5 rounded-md px-1.5 align-middle text-xs" />;
}

/** The keyboard shortcuts a review says would change, each with what the binding would be and is now. */
export function shortcutChangeValues(t: Translate, changed: ReturnType<typeof shortcutsReview>["changed"]): SettingValue[] {
  return changed.map(({ label, binding, current }) => ({ item: t(label), value: shown(<SmallKbd binding={binding} />, <SmallKbd binding={current} />) }));
}

/** The keyboard shortcuts of an applied set of overrides, in the order the commands are listed. */
export function shortcutValues(t: Translate, overrides: Overrides): SettingValue[] {
  return REBINDABLE_COMMANDS
    .filter((command) => overrides[command.id])
    .map((command) => ({ item: t(command.label), value: <SmallKbd binding={overrides[command.id]!} /> }));
}
