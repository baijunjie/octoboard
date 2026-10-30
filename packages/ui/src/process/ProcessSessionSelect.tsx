import type React from "react";

import { PaneToolbar } from "../components/PaneToolbar";
import { OptionSelect, type Option } from "../dialogs/OptionSelect";
import { useT } from "../i18n/react";
import { sessionDisplayTitle } from "../sessionLabel";
import { useProjectSessions } from "../sessions/useSessions";

/** The session filter of a project's Processes panel. Empty means every session's processes, shown as the
 * placeholder rather than as a choice; picking a session narrows the list, and the clear button goes back
 * to all of them. An archived session is offered only while it is the one filtered on. */
export function ProcessSessionSelect({ projectId, value, onChange, landing }: {
  projectId: string;
  landing: boolean;
  value: string | undefined;
  onChange: (id: string | undefined) => void;
}): React.ReactElement {
  const t = useT();
  const sessions = useProjectSessions(projectId);
  const options: Option<string>[] = sessions
    .filter((session) => session.status !== "archived" || session.id === value)
    .map((session) => ({ value: session.id, label: sessionDisplayTitle(t, session) }));
  return (
    <PaneToolbar>
      <OptionSelect hideLabel landing={landing} label={t("processes.sessionFilter")} placeholder={t("processes.allSessions")} options={options} value={value ?? null} onChange={(id: string | null) => onChange(id ?? undefined)} />
    </PaneToolbar>
  );
}
