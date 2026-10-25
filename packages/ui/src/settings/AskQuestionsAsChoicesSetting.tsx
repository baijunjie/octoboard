import React from "react";

import { errorPresentation } from "../errorPresentation";
import { useT } from "../i18n/react";
import { useDaemon, useDaemonStore } from "../store";
import { SwitchSettingRow } from "./SettingRow";

/** Whether sessions are asked to put a question that needs the user's decision as options to pick
 * from (see `Settings.ask_questions_as_choices` in protocol.ts). The row's control never unmounts on
 * its own, so this needs no `useSectionRefocus`. */
export function AskQuestionsAsChoicesSetting(): React.ReactElement {
  const t = useT();
  const { request, toastError } = useDaemon();
  const enabled = useDaemonStore((s) => s.settings.ask_questions_as_choices ?? true);
  const description = t(enabled ? "settings.askQuestionsAsChoices.description.on" : "settings.askQuestionsAsChoices.description.off");

  const setEnabled = (value: boolean) =>
    void request({ type: "update_settings", ask_questions_as_choices: value }).catch((err) => toastError(errorPresentation(err as Error)));

  return (
    <SwitchSettingRow label={t("settings.askQuestionsAsChoices.label")} description={description} isSelected={enabled} onChange={setEnabled} />
  );
}
