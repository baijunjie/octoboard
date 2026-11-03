import React, { useEffect, useRef } from "react";

import { useLatest } from "../components/hooks/useLatest";
import { useFocusHandoff } from "../components/focus/useFocusHandoff";
import { PaneToolbar } from "../components/panel/PaneToolbar";
import { OptionSelect, type Option } from "../components/form/OptionSelect";
import { useT } from "../i18n/react";
import { sessionDisplayTitle } from "../sessions/sessionLabel";
import { useProjectSessions } from "../store";

/** The session filter of a project's Processes panel. Empty means every session's processes, shown as the
 * placeholder rather than as a choice; picking a session narrows the list, and the clear button goes back
 * to all of them. An archived session is offered only while it is the one filtered on. */
export function ProcessSessionSelect({ projectId, value, onChange, landing, shown, opened, onEmptyBlur, toggleRef }: {
  projectId: string;
  landing: boolean;
  shown: boolean;
  opened: boolean;
  onEmptyBlur: () => void;
  toggleRef: React.RefObject<HTMLButtonElement | null>;
  value: string | undefined;
  onChange: (id: string | undefined) => void;
}): React.ReactElement {
  const t = useT();
  const toolbar = useRef<HTMLDivElement>(null);
  const current = useLatest({ value, onEmptyBlur });
  const handoff = useFocusHandoff(() => toggleRef.current?.focus());
  useEffect(() => {
    if (opened) toolbar.current?.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]')?.focus();
  }, [opened]);
  const onBlur = () => {
    // The select's portalled listbox takes focus outside this toolbar. Wait for its closing focus
    // restoration before deciding whether the user left an empty field.
    requestAnimationFrame(() => {
      const element = toolbar.current;
      if (!element || !document.hasFocus() || current.current.value !== undefined) return;
      if (element.contains(document.activeElement) || element.querySelector('[aria-expanded="true"]')) return;
      current.current.onEmptyBlur();
    });
  };
  const sessions = useProjectSessions(projectId);
  const options: Option<string>[] = sessions
    .filter((session) => session.status !== "archived" || session.id === value)
    .map((session) => ({ value: session.id, label: sessionDisplayTitle(t, session) }));
  return (
    <div className="contents" {...handoff}>
      {shown && <PaneToolbar ref={toolbar} onBlur={onBlur}>
        <OptionSelect hideLabel landing={landing} label={t("processes.sessionFilter")} placeholder={t("processes.allSessions")} options={options} value={value ?? null} onChange={(id: string | null) => onChange(id ?? undefined)} />
      </PaneToolbar>}
    </div>
  );
}
