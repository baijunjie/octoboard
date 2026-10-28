import { Description } from "@heroui/react";
import React, { useRef, useState } from "react";

import type { LaunchPair } from "../agents";
import { effortLevels, launchSettingOptions } from "../agentLaunchSettings";
import { FailureBody } from "../components/errorParts";
import { StatusMark } from "../components/StatusMark";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { daemonMessage } from "../daemonMessage";
import { DaemonRequestError } from "../daemon-client";
import { failureSummaryText } from "../errorText";
import type { Translate } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, AgentModel } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { AdvancedSection, type AdvancedTab, AdvancedTabs } from "./AdvancedSection";
import { OptionSelect } from "./OptionSelect";
import { useAgentModels } from "./useAgentModels";

/** The name a listed model is shown under. Claude Code's `best` alias is named in Octoboard's own
 * words by the daemon, so it comes from the catalogue; every other name is the product's or the
 * agent's own and is shown as sent. */
function modelName(t: Translate, model: AgentModel): string {
  return model.id === "best" ? t("dialog.agentSettings.modelBest") : model.name;
}

/** What the launch pickers of one agent and account need to know of the models it offers, asked of
 * the daemon while `enabled` (see `useAgentModels`). While the answer is out the pickers are locked,
 * showing what is stored; when the daemon has no list the model picker offers the stored value alone
 * and `modelFailure` says why, which the picker draws as a warning mark after the field's label (the
 * dialog goes on without the list). `listed` is whether the list is known, which is when a stored value
 * missing from it is marked "Not in the list": not while it loads, and not when it could not be read. `modelText` is what the status region speaks: the loading note, or the failure's
 * sentence alone as plain text. */
export function useLaunchModels(agent: Agent, account: string | null, enabled: boolean) {
  const t = useT();
  const language = useCurrentLanguage();
  const { request } = useDaemon();
  const { status, models, unavailable, error } = useAgentModels(request, agent, account, enabled);
  const loading = status === "loading";
  // Selected as the worded text rather than the records it names, so the field follows a rename
  // without re-rendering on every other change to those records.
  const unavailableText = useDaemonStore((s) =>
    unavailable ? daemonMessage(language, unavailable.code, unavailable.params, unavailable.message, s) : undefined,
  );
  // Why there are no models, with what it names set as code where the message has such params.
  const reason = unavailable ? new DaemonRequestError(unavailableText ?? unavailable.message, unavailable.code, unavailable.params) : error;
  // The region says the sentence the mark's popover opens with; the raw message, the remedy and the
  // details belong in the popover.
  const reasonText = useDaemonStore((s) => (reason === undefined ? undefined : failureSummaryText(t, language, reason, s)));
  const modelText = loading ? t("dialog.agentSettings.modelsLoading") : reasonText;
  const modelDescription = loading ? modelText : undefined;
  const modelFailure = loading ? undefined : reason;
  const listed = status === "ready" && !unavailable;
  return { loading, listed, models, modelText, modelDescription, modelFailure };
}

export type LaunchModels = ReturnType<typeof useLaunchModels>;

/** What an unset picker says in place of "unset": that it inherits, with a badge naming the value it
 * resolves to, or the agent's own setting while that is none. */
function inherited(t: Translate, value: string | null): { placeholder: string; placeholderBadge: string } {
  return {
    placeholder: t("dialog.agentSettings.sameAsConsole"),
    placeholderBadge: value || t("dialog.agentSettings.inheritOwn"),
  };
}

/** The warning mark of a Model field whose list of models could not be read: it concerns the whole field,
 * so it stands after the field's label. Nothing when the list is there. `markId` is the id the field's
 * focus hand-off knows it by (`OptionSelect`'s `labelMark`). */
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
  inherit?: LaunchPair;
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

/** The Model picker of one pair of settings, empty while unset. It carries the note on the model list
 * while it loads; why there is none is `LaunchModels.modelFailure`, a warning mark after the field's label
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
  const listed = launch.models.find((candidate) => candidate.id === inherit?.model);
  const inheritedText = inherit && inherited(t, listed ? modelName(t, listed) : inherit.model);
  return (
    <OptionSelect
      label={t("dialog.agentSettings.model")}
      hideLabel={hideLabel}
      labelledBy={labelledBy}
      options={launchSettingOptions(
        t,
        launch.models.map((listed) => ({ value: listed.id, label: modelName(t, listed) })),
        model,
        launch.listed,
      )}
      value={model}
      onChange={onChange}
      placeholder={inheritedText?.placeholder ?? t("dialog.agentSettings.placeholder")}
      placeholderBadge={inheritedText?.placeholderBadge}
      isDisabled={isDisabled || launch.loading}
      description={launch.modelDescription}
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
  const { levels, none } = effortLevels(launch.models, model ?? inherit?.model ?? null);
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
      isDisabled={isDisabled || launch.loading}
      description={none ? t("dialog.agentSettings.noEffort") : undefined}
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
  inherit?: LaunchPair;
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
  inherit?: LaunchPair;
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
  // A reload (the account the form refers to going away elsewhere) takes the Model field's mark away and locks
  // its picker in the same render, so focus that was on the mark has no enabled picker to go to: it goes to the
  // Model picker if that is operable, and else to the selected tab, where the section has tabs, or the section's
  // trigger, both of which stay operable.
  const focusHandoff = useFocusHandoff(() => {
    const modelTrigger = pickers.current?.querySelector<HTMLElement>("[data-slot=select-trigger]");
    const section = pickers.current?.closest("[data-slot=disclosure]");
    const target = modelTrigger && !modelTrigger.matches(":disabled")
      ? modelTrigger
      : (section?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]') ??
        section?.querySelector<HTMLElement>("[data-slot=disclosure-trigger]"));
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
