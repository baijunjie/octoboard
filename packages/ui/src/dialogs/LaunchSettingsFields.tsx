import { Description, Disclosure } from "@heroui/react";
import React, { useState } from "react";

import { effortLevels, launchSettingOptions } from "../agentLaunchSettings";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { daemonMessage } from "../daemonMessage";
import type { Translate } from "../i18n/catalog";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Agent, AgentModel } from "../protocol";
import { useDaemon } from "../store";
import { OptionSelect } from "./OptionSelect";
import { useAgentModels } from "./useAgentModels";

/** The name a listed model is shown under. Claude Code's `best` alias is named in Octoboard's own
 * words by the daemon, so it comes from the catalogue; every other name is the product's or the
 * agent's own and is shown as sent. */
function modelName(t: Translate, model: AgentModel): string {
  return model.id === "best" ? t("dialog.agentSettings.modelBest") : model.name;
}

/** The collapsed Advanced section of a dialog: the model and reasoning effort that `agent`'s
 * sessions are launched with, each empty while unset. `note` says whose sessions they are. Folded
 * away by default, as what is inside is rarely set. `account` is the account id the agent's
 * sessions run under (`null` for its default account), as the list of models belongs to the account.
 * That list is asked of the daemon while the section is open, and again when `agent` or `account`
 * changes, so nothing is asked of a dialog whose Advanced section is never opened. While the answer
 * is out the pickers are locked, showing what is stored; when the daemon has no list the model
 * picker offers the stored value alone and says why, and the effort picker is left to the stored
 * value too, which a failure does not stop the user from clearing. A model that takes no effort
 * leaves the effort picker operable over an empty list, so a stored value stays clearable and
 * focus is never on a control that turns disabled under it. `isDisabled` locks the two
 * pickers; the trigger stays operable, as folding the section away changes nothing and disabling a
 * focused trigger would blur it. */
export function LaunchSettingsFields({
  agent,
  account,
  model,
  effort,
  onModelChange,
  onEffortChange,
  note,
  isDisabled,
}: {
  agent: Agent;
  account: string | null;
  model: string | null;
  effort: string | null;
  onModelChange: (value: string | null) => void;
  onEffortChange: (value: string | null) => void;
  note: string;
  isDisabled?: boolean;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, store } = useDaemon();
  const [expanded, setExpanded] = useState(false);
  const { status, models, unavailable, error } = useAgentModels(request, agent, account, expanded);
  const loading = status === "loading";

  const modelDescription = loading
    ? t("dialog.agentSettings.modelsLoading")
    : unavailable
      ? daemonMessage(language, unavailable.code, unavailable.params, unavailable.message, store.getState())
      : error;
  const { levels, none } = effortLevels(models, model);

  return (
    <Disclosure isExpanded={expanded} onExpandedChange={setExpanded}>
      {/* The model field's note changes with no action of the user's, and its picker is locked while
          the note says "loading", so it cannot be reached to be read: say it from a status region. */}
      <StatusAnnouncer text={modelDescription} />
      <Disclosure.Heading>
        <Disclosure.Trigger className="flex w-full items-center">
          {t("dialog.agentSettings.advanced")}
          <Disclosure.Indicator />
        </Disclosure.Trigger>
      </Disclosure.Heading>
      {/* HeroUI pads the body by 8px on every side and the content clips what overflows it, so that
          padding is the room the pickers' focus rings need. Taking all of it out as a margin would
          reach past the dialog body, whose own slack is 4px, so the content takes 4px of it back
          and the body's inner element the other 4px, which lands the pickers in line with the
          fields above with 2px of ring to spare inside the clip. The padding left above the
          first note and below the last picker is HeroUI's own, the same on both sides. */}
      <Disclosure.Content className="-mx-1">
        <Disclosure.Body className="-mx-1 flex flex-col gap-4">
          <Description>{note}</Description>
          <OptionSelect
            label={t("dialog.agentSettings.model")}
            options={launchSettingOptions(
              t,
              models.map((listed) => ({ value: listed.id, label: modelName(t, listed) })),
              model,
            )}
            value={model}
            onChange={onModelChange}
            placeholder={t("dialog.agentSettings.placeholder")}
            isDisabled={isDisabled || loading}
            description={modelDescription}
          />
          <OptionSelect
            label={t("dialog.agentSettings.effort")}
            options={launchSettingOptions(t, levels.map((level) => ({ value: level, label: level })), effort)}
            value={effort}
            onChange={onEffortChange}
            placeholder={t("dialog.agentSettings.placeholder")}
            isDisabled={isDisabled || loading}
            description={none ? t("dialog.agentSettings.noEffort") : undefined}
          />
        </Disclosure.Body>
      </Disclosure.Content>
    </Disclosure>
  );
}
