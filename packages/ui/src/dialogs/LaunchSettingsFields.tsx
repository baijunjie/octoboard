import { Description, Disclosure } from "@heroui/react";
import React, { useState } from "react";

import type { LaunchPair } from "../agents";
import { effortLevels, launchSettingOptions } from "../agentLaunchSettings";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { daemonMessage } from "../daemonMessage";
import { DaemonRequestError } from "../daemon-client";
import { errorText } from "../errorText";
import type { Translate } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, AgentModel } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
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
 * and says why. */
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
  const modelText = loading ? t("dialog.agentSettings.modelsLoading") : (unavailableText ?? (error !== undefined ? errorText(error) : undefined));
  // Why there are no models, with what it names set as code where the message has such params.
  const reason = unavailable ? new DaemonRequestError(unavailableText ?? unavailable.message, unavailable.code, unavailable.params) : error;
  const modelDescription = !loading && reason !== undefined ? <DaemonErrorMessage error={reason} part="summary" /> : modelText;
  const modelDetails = !loading && reason !== undefined ? <DaemonErrorMessage error={reason} part="details" /> : undefined;
  return { loading, models, modelText, modelDescription, modelDetails };
}

export type LaunchModels = ReturnType<typeof useLaunchModels>;

/** What an unset picker says in place of "unset": that it inherits, with a badge naming the value it
 * resolves to, or the agent's own setting while that is none. */
function inherited(t: Translate, value: string | null): { placeholder: string; placeholderBadge: string } {
  return {
    placeholder: t("dialog.agentSettings.inherit"),
    placeholderBadge: value || t("dialog.agentSettings.inheritOwn"),
  };
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
 * (loading, or why there is none). */
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
      )}
      value={model}
      onChange={onChange}
      placeholder={inheritedText?.placeholder ?? t("dialog.agentSettings.placeholder")}
      placeholderBadge={inheritedText?.placeholderBadge}
      isDisabled={isDisabled || launch.loading}
      description={launch.modelDescription}
      descriptionDetails={launch.modelDetails}
      describedBy={describedBy}
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
      options={launchSettingOptions(t, levels.map((level) => ({ value: level, label: level })), effort)}
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
function LaunchPickers({
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

/** The collapsed Advanced section of a dialog, folded away by default as what is inside is rarely
 * set. The trigger spans the row so the whole row expands the section; HeroUI pushes its indicator
 * to the far end with an auto margin, which `ms-0` cancels so the chevron follows the label. The
 * label is set like the labels of the fields around it (`Label`'s size, weight and colour). */
export function AdvancedSection({
  expanded,
  onExpandedChange,
  status,
  children,
}: {
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
  /** The model field's note, said from a status region: it changes with no action of the user's,
   * and its picker is locked while it says "loading", so it cannot be reached to be read. */
  status?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  return (
    <Disclosure isExpanded={expanded} onExpandedChange={onExpandedChange}>
      <Disclosure.Heading>
        <Disclosure.Trigger className="flex w-full items-center gap-1 text-sm font-medium text-foreground">
          {t("dialog.agentSettings.advanced")}
          <Disclosure.Indicator className="ms-0" />
        </Disclosure.Trigger>
      </Disclosure.Heading>
      {/* Outside the folded body, and only while the section is open. The body stays mounted while
          folded, so a region inside it would be past its first write delay by the time the section
          opens again, and the note kept from before would be written in the very render that shows
          the body: text that appears with its region, which VoiceOver does not say. Mounted with
          each opening, the region writes the note after its delay, with the trigger still focused. */}
      {expanded && <StatusAnnouncer text={status} />}
      {/* HeroUI pads the body by 8px on every side and the content clips what overflows it, so that
          padding is the room the fields' focus rings need. Taking all of it out as a margin would
          reach past the dialog body, whose own slack is 4px, so the content takes 4px of it back
          and the body's inner element the other 4px, which lands the pickers in line with the
          fields above with 2px of ring to spare inside the clip. The padding left above the
          first note and below the last field is HeroUI's own, the same on both sides. */}
      <Disclosure.Content className="-mx-1">
        <Disclosure.Body className="-mx-1 flex flex-col gap-4">
          {children}
        </Disclosure.Body>
      </Disclosure.Content>
    </Disclosure>
  );
}

/** The Advanced section of a dialog about one agent's sessions: the model and reasoning effort that
 * `agent`'s sessions are launched with, each empty while unset. `note` says whose sessions they
 * are. `account` is the account id the agent's sessions run under (`null` for its default
 * account), as the list of models belongs to the account. That list is asked of the daemon while
 * the section is open, and again when `agent` or `account` changes, so nothing is asked of a
 * dialog whose Advanced section is never opened (see `useLaunchModels`). `isDisabled` locks the two
 * pickers, not `children`; the trigger stays operable, as folding the section away changes nothing
 * and disabling a focused trigger would blur it. `children` are further rarely set fields of the
 * same dialog, placed below the two pickers. */
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
  children,
}: {
  agent: Agent;
  account: string | null;
  model: string | null;
  effort: string | null;
  onModelChange: (value: string | null) => void;
  onEffortChange: (value: string | null) => void;
  note: string;
  inherit?: LaunchPair;
  isDisabled?: boolean;
  children?: React.ReactNode;
}): React.ReactElement {
  const [expanded, setExpanded] = useState(false);
  const launch = useLaunchModels(agent, account, expanded);
  return (
    <AdvancedSection expanded={expanded} onExpandedChange={setExpanded} status={launch.modelText}>
      <Description>{note}</Description>
      <LaunchPickers
        launch={launch}
        model={model}
        effort={effort}
        onModelChange={onModelChange}
        onEffortChange={onEffortChange}
        inherit={inherit}
        isDisabled={isDisabled}
      />
      {children}
    </AdvancedSection>
  );
}
