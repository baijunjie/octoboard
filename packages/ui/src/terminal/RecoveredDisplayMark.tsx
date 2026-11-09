import React from "react";

import { StatusMark } from "../components/status/StatusMark";
import { isolate } from "../i18n/isolate";
import { useT } from "../i18n/react";

/** What the mark and its popover carry as `data-status-mark-id`, for a handoff of focus held in either. */
export const RECOVERED_DISPLAY_MARK_ID = "recovered-display";

/**
 * The note mark beside the title of a terminal whose screen a restarted daemon rebuilt
 * (`screen_inexact`): not a status of the session, and it follows the flag, so a redraw does not
 * clear it. `terminalTitle` names the terminal where the mark does not stand right after its title
 * (the shell's tab strip), so the mark's accessible name says which terminal it is about.
 */
export function RecoveredDisplayMark({ terminalTitle }: { terminalTitle?: string }): React.ReactElement {
  const t = useT();
  const title = t("terminal.recovered.title");
  return (
    <StatusMark tone="note" markId={RECOVERED_DISPLAY_MARK_ID} label={terminalTitle === undefined ? title : t("shellTerminal.recovered.label", { display: title, title: isolate(terminalTitle) })}>
      <p>{t("terminal.recovered.body")}</p>
    </StatusMark>
  );
}
