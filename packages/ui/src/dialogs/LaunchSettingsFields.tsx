import React, { useRef, useState } from "react";

import { effortLevels, type LaunchSettingOption, launchSettingOptions, shownModel } from "../agents/agentLaunchSettings";
import { providerModelDisplay } from "../agents/providers";
import { FailureBody } from "../components/alerts/errorParts";
import { StatusMark } from "../components/status/StatusMark";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { daemonMessage } from "../errors/daemonMessage";
import { CodedError } from "../errors/codedError";
import { failureSummaryText } from "../errors/errorText";
import type { Translate } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, AgentModel, Launch } from "../protocol";
import { modelDisplayName } from "../sessions/sessionModel";
import { useDaemon, useDaemonStore, useProviders } from "../store";
import { AdvancedSection, type AdvancedTab, AdvancedTabs } from "../components/form/AdvancedSection";
import { OptionSelect } from "../components/form/OptionSelect";
import { useAgentModels } from "./useAgentModels";
import { Description, partSelector } from "../primitives";

/** A listed model as a picker option: its name, with the concrete version it stands for under it
 * where the daemon knows it, and an "Unavailable" badge where the account cannot use it. The
 * version is also the option's description for assistive technology, which the line drawn under the
 * name is hidden from. */
function modelOption(t: Translate, model: AgentModel): LaunchSettingOption {
  const badge = model.available === false ? { badge: t("dialog.agentSettings.modelUnavailable") } : {};
  if (!model.version) return { value: model.id, label: model.name, ...badge };
  return { value: model.id, label: model.name, ...badge, detail: model.version, reason: { state: model.version } };
}

/** What the launch pickers of one agent and account need to know of the models it offers, asked of the
 * daemon while `enabled` (see `useAgentModels`). While the first answer for the agent and account is out
 * the pickers are locked, showing stored values or a loading placeholder; when the daemon has no list the model
 * picker offers the stored value alone and `modelFailure` says why, which the picker draws as a warning mark after the
 * field's label (the dialog goes on without the list). `listed` is whether the list is known, which is
 * when a stored value missing from it is marked "Not in the list": not while it loads, not when it could
 * not be read, and not when it came back empty, which the daemon treats as no list. `modelText` is what
 * the status region speaks: the loading note, or the failure's sentence alone as plain text. */
export function useLaunchModels(agent: Agent, account: string | null, enabled: boolean) {
  const t = useT();
  const language = useCurrentLanguage();
  const { request } = useDaemon();
  const display = providerModelDisplay(useProviders(), agent);
  const { status, models, defaultModel, defaultModelSource, unavailable, error } = useAgentModels(request, agent, account, enabled);
  const loading = status === "loading";
  // Selected as the worded text rather than the records it names, so the field follows a rename
  // without re-rendering on every other change to those records.
  const unavailableText = useDaemonStore((s) =>
    unavailable ? daemonMessage(language, unavailable.code, unavailable.params, unavailable.message, s) : undefined,
  );
  // Why there are no models, with what it names set as code where the message has such params.
  const reason = unavailable ? new CodedError(unavailableText ?? unavailable.message, unavailable.code, unavailable.params) : error;
  // The region says the sentence the mark's popover opens with; the raw message, the remedy and the
  // details belong in the popover.
  const reasonText = useDaemonStore((s) => (reason === undefined ? undefined : failureSummaryText(t, language, reason, s)));
  const modelText = loading ? t("dialog.agentSettings.modelsLoading") : reasonText;
  const modelFailure = loading ? undefined : reason;
  const listed = status === "ready" && !unavailable && models.length > 0;
  return { agent, display, loading, listed, models, defaultModel, defaultModelSource, modelText, modelFailure };
}

export type LaunchModels = ReturnType<typeof useLaunchModels>;

/** What an unset picker says in place of "unset": that it inherits, with a badge naming the value it
 * resolves to, or that the setting is the owner's own while that is none. */
function inherited(t: Translate, value: string | null): { placeholder: string; placeholderBadge: string } {
  return {
    placeholder: t("dialog.agentSettings.sameAsConsole"),
    placeholderBadge: value || t("dialog.agentSettings.inheritOwn"),
  };
}

/** The model a pair resolves to: its own `model` if it names one, else the one it `inherit`s, else the
 * one Octoboard launches with when nothing names one, else `null`, the agent's own choice. */
function resolvedModel(launch: LaunchModels, model: string | null, inherit: Launch | undefined): string | null {
  return shownModel(launch.display, model) ?? shownModel(launch.display, inherit?.model ?? null) ?? launch.defaultModel;
}

/** The warning mark of a Model field whose list of models could not be read: it concerns the whole field,
 * so it stands after the field's label. Nothing when the list is there. `markId` is the id the field's
 * focus handoff knows it by (`OptionSelect`'s `labelMark`). */
export function ModelsFailureMark({ launch, markId }: { launch: LaunchModels; markId?: string }): React.ReactElement | null {
  const t = useT();
  const failure = launch.modelFailure;
  if (failure === undefined) return null;
  return (
    <StatusMark tone="warning" inline markId={markId} label={t("dialog.agentSettings.modelsFailedMark")}>
      <FailureBody error={failure} />
    </StatusMark>
  );
}

/** The props `ModelPicker` and `EffortPicker` have in common. */
type PickerProps = {
  launch: LaunchModels;
  /** The values an unset picker resolves to (the console's, for a project): an unset picker names
   * them instead of standing for the agent's own setting. */
  inherit?: Launch;
  isDisabled?: boolean;
  /** The id of an element elsewhere on the page that also describes the picker. */
  describedBy?: string;
} & (
  | { hideLabel?: undefined; labelledBy?: undefined }
  | {
    /** Leaves the label off the screen, for a picker that a row or column header already names. */
    hideLabel: true;
    /** The ids of those headers, which name the picker in the order given in place of its label. */
    labelledBy: string;
  }
);

/** The Model picker of one pair of settings, empty while unset. While its options load it shows a spinner and a loading
 * placeholder; why there is none is `LaunchModels.modelFailure`, a warning mark after the field's label
 * (`ModelsFailureMark`, which a caller that hides the label places beside its own). */
export function ModelPicker({
  launch,
  model,
  onChange,
  inherit,
  isDisabled,
  hideLabel,
  labelledBy,
  describedBy,
}: PickerProps & { model: string | null; onChange: (value: string | null) => void }): React.ReactElement {
  const t = useT();
  // A badge names the concrete version where the list knows it ("Fable 5.1"), as the options do; an
  // id the list lacks (a model Claude Code's own configuration names) is read as a session's is.
  const nameOf = (id: string | null) => {
    if (id === null) return null;
    const listed = launch.models.find((candidate) => candidate.id === id);
    return listed?.version ?? listed?.name ?? modelDisplayName(id, launch.display);
  };
  const shown = shownModel(launch.display, model);
  // What an unset picker resolves to, for its badge.
  const resolved = nameOf(resolvedModel(launch, null, inherit));
  let unset: { placeholder: string; placeholderBadge?: string } | undefined;
  if (inherit) unset = inherited(t, resolved);
  else if (launch.defaultModel !== null) {
    // A model Claude Code's own configuration names is its own setting, not Octoboard's best pick.
    const placeholder = launch.defaultModelSource === "configured" ? "dialog.agentSettings.placeholder" : "dialog.agentSettings.modelDefault";
    unset = { placeholder: t(placeholder), placeholderBadge: resolved ?? undefined };
  }
  return (
    <OptionSelect
      label={t("dialog.agentSettings.model")}
      hideLabel={hideLabel}
      labelledBy={labelledBy}
      options={launchSettingOptions(t, launch.models.map((listed) => modelOption(t, listed)), shown, launch.listed)}
      value={shown}
      onChange={onChange}
      placeholder={unset?.placeholder ?? t("dialog.agentSettings.placeholder")}
      placeholderBadge={unset?.placeholderBadge}
      isDisabled={isDisabled}
      isLoading={launch.loading}
      describedBy={describedBy}
      labelMark={launch.modelFailure === undefined ? undefined : (markId) => <ModelsFailureMark launch={launch} markId={markId} />}
    />
  );
}

/** The Reasoning effort picker of one pair of settings, empty while unset. The levels follow `model`,
 * the pair's model, or the one it resolves to while none is chosen. A model that takes no effort
 * leaves the picker operable over an empty list, so a stored value stays clearable and focus is
 * never on a control that turns disabled under it. */
export function EffortPicker({
  launch,
  model,
  effort,
  onChange,
  inherit,
  isDisabled,
  hideLabel,
  labelledBy,
  describedBy,
}: PickerProps & { model: string | null; effort: string | null; onChange: (value: string | null) => void }): React.ReactElement {
  const t = useT();
  const { levels, none } = effortLevels(launch.models, resolvedModel(launch, model, inherit));
  const inheritedText = inherit && inherited(t, inherit.effort);
  return (
    <OptionSelect
      label={t("dialog.agentSettings.effort")}
      hideLabel={hideLabel}
      labelledBy={labelledBy}
      options={launchSettingOptions(t, levels.map((level) => ({ value: level, label: level })), effort, launch.listed)}
      value={effort}
      onChange={onChange}
      placeholder={inheritedText?.placeholder ?? t("dialog.agentSettings.placeholder")}
      placeholderBadge={inheritedText?.placeholderBadge}
      isDisabled={isDisabled}
      isLoading={launch.loading}
      description={!launch.loading && none ? t("dialog.agentSettings.noEffort") : undefined}
      describedBy={describedBy}
    />
  );
}

/** The Model and Reasoning effort pickers of one pair of settings. */
export function LaunchPickers({
  launch,
  model,
  effort,
  onModelChange,
  onEffortChange,
  inherit,
  isDisabled,
}: {
  launch: LaunchModels;
  model: string | null;
  effort: string | null;
  onModelChange: (value: string | null) => void;
  onEffortChange: (value: string | null) => void;
  inherit?: Launch;
  isDisabled?: boolean;
}): React.ReactElement {
  return (
    <>
      <ModelPicker launch={launch} model={model} onChange={onModelChange} inherit={inherit} isDisabled={isDisabled} />
      <EffortPicker launch={launch} model={model} effort={effort} onChange={onEffortChange} inherit={inherit} isDisabled={isDisabled} />
    </>
  );
}

/** The Advanced section of a dialog about one agent's sessions: the model and reasoning effort that
 * `agent`'s sessions are launched with, each empty while unset. `note` explains how the settings are
 * inherited where needed. `account` is the account id the agent's sessions run under (`null` for its
 * default account), as the list of models belongs to the account. That list is asked of the daemon while
 * the section is open, and again when `agent` or `account` changes, so nothing is asked of a
 * dialog whose Advanced section is never opened (see `useLaunchModels`). `isDisabled` locks the two
 * pickers; the trigger stays operable, as folding the section away changes nothing and disabling a
 * focused trigger would blur it. `otherTab` is a further group of rarely set fields of the same
 * dialog, a kind of setting of its own: with it the section holds an `AdvancedTabs` of two tabs, the
 * pickers under the first (with `note`) and the group under the second, so the group does not read
 * as one more field beside Model and Reasoning effort. */
export function LaunchSettingsFields({
  agent,
  account,
  model,
  effort,
  onModelChange,
  onEffortChange,
  note,
  inherit,
  isDisabled,
  expanded: controlledExpanded,
  onExpandedChange,
  otherTab,
}: {
  agent: Agent;
  account: string | null;
  model: string | null;
  effort: string | null;
  onModelChange: (value: string | null) => void;
  onEffortChange: (value: string | null) => void;
  note?: string;
  inherit?: Launch;
  isDisabled?: boolean;
  /** Whether the section is open, for a dialog that keeps it open across a swap of this component for another
   * `AdvancedSection`; without it the section keeps its own state. */
  expanded?: boolean;
  onExpandedChange?: (expanded: boolean) => void;
  /** The second tab, whose fields `isDisabled` leaves alone. */
  otherTab?: AdvancedTab;
}): React.ReactElement {
  const t = useT();
  const [ownExpanded, setOwnExpanded] = useState(false);
  const expanded = controlledExpanded ?? ownExpanded;
  const setExpanded = onExpandedChange ?? setOwnExpanded;
  const launch = useLaunchModels(agent, account, expanded);
  const pickers = useRef<HTMLDivElement>(null);
  // A reload (the account the form refers to going away elsewhere) takes the Model field's mark away and locks its
  // picker in the same render, so focus that was on the mark has no enabled picker to go to: it goes to the Model
  // picker if that is operable, and else to the selected tab, where the section has tabs, or the section's trigger,
  // both of which stay operable outside a busy dialog.
  const focusHandoff = useFocusHandoff(() => {
    const modelTrigger = pickers.current?.querySelector<HTMLElement>(partSelector.selectTrigger);
    const section = pickers.current?.closest(partSelector.disclosure);
    const target = modelTrigger && !modelTrigger.matches(":disabled")
      ? modelTrigger
      : (section?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ??
        section?.querySelector<HTMLElement>(partSelector.disclosureTrigger));
    target?.focus();
  });
  const launchFields = (
    <>
      {note && <Description>{note}</Description>}
      <div ref={pickers} {...focusHandoff} className="flex flex-col gap-4">
        <LaunchPickers
          launch={launch}
          model={model}
          effort={effort}
          onModelChange={onModelChange}
          onEffortChange={onEffortChange}
          inherit={inherit}
          isDisabled={isDisabled}
        />
      </div>
    </>
  );
  return (
    <AdvancedSection expanded={expanded} onExpandedChange={setExpanded} status={launch.modelText}>
      {otherTab ? (
        <AdvancedTabs
          tabs={[
            { id: "launch", label: t("dialog.agentSettings.launchTab"), content: launchFields },
            otherTab,
          ]}
        />
      ) : launchFields}
    </AdvancedSection>
  );
}
