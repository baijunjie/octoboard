import React from "react";

import { idWords, servedProvider, settingMessageKey } from "../agents/providers";
import { Tag } from "../components/form/Tag";
import { joinNodes } from "../i18n/joinPhrases";
import type { Language } from "../i18n/languages";
import { DELIVERY_CHOICES, DEVELOPMENT_CHOICES, GIT_WORKFLOW_ITEMS, GIT_WORKFLOW_ITEM_KEYS } from "../git/gitWorkflow";
import type { MessageKey, PlainMessageKey, Translate } from "../i18n/catalog";
import { Message } from "../i18n/react";
import type { Agent, GitWorkflowSettings, InspectedAgentSettings, ProviderInfo, SettingDescriptor } from "../protocol";
import type { Binding } from "../shortcuts/binding";
import { REBINDABLE_COMMANDS } from "../shortcuts/commands";
import type { Overrides } from "../shortcuts/store";
import { BindingKbd } from "../shortcuts/ShortcutKbd";
import { sameValue } from "../values/sameValue";
import type { SessionOrder } from "../sessions/order";
import { SESSION_ORDER_PARTS, sortLabel } from "../sessions/orderWording";
import type { shortcutsReview } from "./importPlan";

/** One value of a composite setting, listed under its line of the import cards: the item and what it is. A line that
 * is about one thing already (a setting of the agent whose card it is in) has no `item`, only its value. */
export interface SettingValue {
  item?: string;
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

/** Each part of the session order `wanted` carries with its choice, named and worded as the settings page does. With
 * `current`, only the parts that differ from it, each with what it is now. */
export function sessionOrderValues(t: Translate, wanted: Partial<SessionOrder>, current?: SessionOrder): SettingValue[] {
  return SESSION_ORDER_PARTS.flatMap(({ field, label }) => {
    const value = wanted[field];
    if (value === undefined || (current && value === current[field])) return [];
    return [{ item: t(label), value: shown(sortLabel(t, value), current && sortLabel(t, current[field])) }];
  });
}

/** The values in effect on this machine of the exported provider settings, which a choice left unset is `null` in. */
export type CurrentProviderSettings = InspectedAgentSettings["current_provider_settings"];

/** One line of an agent's card of the import cards for a provider setting: its name, and the agent's value for it. */
export interface ProviderSettingLine {
  /** Identifies the line among its siblings. */
  key: string;
  /** The setting's name. */
  label: string;
  /** The agent's value, or none where it would not change. */
  values: SettingValue[];
}

/** The message `name` of a provider's setting, in plain words, or `undefined` for one with no message. */
function settingWord(t: Translate, agent: string, setting: string, name: string): string | undefined {
  const key = settingMessageKey(agent, setting, name);
  return key && (t as (key: MessageKey) => string)(key);
}

/** Whether `value` is one the setting takes: a switch's on or off, one of a choice's declared values. */
function isSettingValue(descriptor: SettingDescriptor, value: unknown): value is boolean | string {
  return descriptor.kind.type === "switch"
    ? typeof value === "boolean"
    : typeof value === "string" && descriptor.kind.choices.some((choice) => choice.value === value);
}

/** How a setting's value reads in the review: a switch On or Off, a choice by its provider's word for it, and a choice
 * left unset by its provider's inheritance label, else as following the account configuration. */
function settingValueText(t: Translate, agent: string, setting: string, value: boolean | string | null): string {
  if (typeof value === "boolean") return t(onOff(value));
  if (value === null) return settingWord(t, agent, setting, "inherit") ?? t("settings.accounts.setting.inherit");
  return settingWord(t, agent, setting, `choice.${value}`) ?? idWords(value);
}

/**
 * The provider settings a document carries for one agent (`wanted`), one line for each, named by the setting alone as
 * its provider's presentation words it (a setting no presentation has a heading for is named by its id) and holding
 * the agent's value. A setting counts when the provider this daemon serves for `agent` declares it exported and the
 * value is one it takes, as the daemon applies it. With `current`, a line has its value only where applying would
 * change it, with what it is now: a setting this machine does not report has nothing to compare with, so it counts as
 * unchanged.
 */
export function providerSettingLines(
  t: Translate,
  providers: readonly ProviderInfo[],
  agent: Agent,
  wanted: Record<string, boolean | string> | undefined,
  current?: CurrentProviderSettings,
): ProviderSettingLine[] {
  const lines: ProviderSettingLine[] = [];
  for (const descriptor of servedProvider(providers, agent)?.settings ?? []) {
    const value = wanted?.[descriptor.id];
    if (!descriptor.exported || !isSettingValue(descriptor, value)) continue;
    const now = current?.[agent]?.[descriptor.id];
    lines.push({
      key: descriptor.id,
      label: settingWord(t, agent, descriptor.id, "heading") ?? idWords(descriptor.id),
      values: current && (now === undefined || now === value) ? [] : [{ value: shown(settingValueText(t, agent, descriptor.id, value), now === undefined ? undefined : settingValueText(t, agent, descriptor.id, now)) }],
    });
  }
  return lines;
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

/** The tags `wanted` gives `agent`, as the value of its line, or nothing when it gives none. With `current`, nothing
 * either when they are the ones the agent has now, which are none for an agent missing from `current`; else the value
 * with what it has now. */
export function agentTagsValues(t: Translate, language: Language, agent: Agent, wanted: Partial<Record<Agent, string[]>>, current?: Partial<Record<Agent, string[]>>): SettingValue[] {
  const tags = wanted[agent];
  if (!tags || (current && sameValue(tags, current[agent] ?? []))) return [];
  return [{ value: shown(tagsNode(t, language, tags), current && tagsNode(t, language, current[agent] ?? [])) }];
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
