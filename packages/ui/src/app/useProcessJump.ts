import { useEffect, useState } from "react";

import { useStableHandler } from "../components/hooks/useLatest";
import type { AsideOwner } from "../layout/asideOwner";
import type { PanelKind } from "../layout/panelKind";
import { landInPanel } from "../panels/panelLanding";
import type { ProcessJump } from "../process/processJump";
import type { Project } from "../protocol";
import { useDaemon } from "../store";
import type { ProjectPanelKind } from "../sidebar/types";

/** Takes the user from the global process list to the Processes panel that owns a row: its console is made current,
 * and the panel opens for its project (every session's processes) or its console session, with focus sent into it: the
 * user asked for this panel, and the row they pressed is gone. A floating panel lands focus itself as it opens
 * (`useFloatingPanel`); a docked one is landed in here, once React has rendered the console and the panel, and not
 * when a newer jump or the unmount comes first. Tells whether the row's project or session still exists to jump to. */
export function useProcessJump({ asideFloats, switchConsole, openProjectPanel, showPanel }: {
  /** The aside floats over the agent terminal, so opening its panel sends focus into it. */
  asideFloats: boolean;
  switchConsole: (consoleId: string) => void;
  openProjectPanel: (project: Project, kind: ProjectPanelKind) => void;
  showPanel: (owner: AsideOwner, kind: PanelKind) => void;
}) {
  const { store } = useDaemon();
  // Each docked jump asks once, whether or not the panel was already shown.
  const [landing, setLanding] = useState<object>();
  useEffect(() => {
    if (!landing) return;
    // Once React has finished the commit's effects, StrictMode's re-run of them included, not within them: in
    // development that re-run unmounts and mounts the effects again, which hands focus on from a card that has it.
    let cancel: (() => void) | undefined;
    let stale = false;
    queueMicrotask(() => {
      if (!stale) cancel = landInPanel("processes");
    });
    return () => {
      stale = true;
      cancel?.();
    };
  }, [landing]);
  return useStableHandler((target: ProcessJump): boolean => {
    const { projects, sessions } = store.getState();
    if ("consoleSession" in target) {
      const session = sessions.get(target.consoleSession);
      if (!session) return false;
      switchConsole(session.console_id);
      showPanel({ kind: "report", consoleSession: session.id }, "processes");
      if (!asideFloats) setLanding({});
      return true;
    }
    const project = projects.get(target.project);
    if (!project) return false;
    switchConsole(project.console_id);
    openProjectPanel(project, "processes");
    if (!asideFloats) setLanding({});
    return true;
  });
}
