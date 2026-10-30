import { createContext, useContext, useState } from "react";

import { ownerKey, type AsideOwner } from "../layout/asideOwner";
import { useSession } from "../sessions/useSessions";
import { useProcessTrees, type ProcessPolling } from "./useProcessTrees";

/** The project filter follows a changed sidebar selection, while a manual choice lasts until that
 * selection changes again. The owner is part of the key so a previous owner's list is never shown. */
export function useProcessPanel(owner: AsideOwner | undefined, selectedSessionId: string | undefined, polling: ProcessPolling) {
  const key = ownerKey(owner);
  const selected = useSession(selectedSessionId);
  const followed = owner?.kind === "project" && selected?.project_id === owner.project ? selected.id : undefined;
  const [filter, setFilter] = useState({ owner: key, selected: selectedSessionId, session: followed });
  if (filter.owner !== key || filter.selected !== selectedSessionId) setFilter({ owner: key, selected: selectedSessionId, session: followed });
  const candidate = filter.owner === key && filter.selected === selectedSessionId ? filter.session : followed;
  const session = useSession(candidate);
  const projectId = owner?.kind === "project" ? owner.project : undefined;
  const sessionId = owner?.kind === "report" ? owner.consoleSession : session?.project_id === projectId ? candidate : undefined;
  const query = useProcessTrees(owner?.kind === "project" ? { project_id: owner.project } : owner ? { session_id: owner.consoleSession } : undefined, polling);
  return {
    ...query,
    ownerKey: key,
    projectId,
    sessionId,
    trees: sessionId === undefined ? query.trees : query.trees?.filter((tree) => tree.session_id === sessionId),
    selectSession: (id: string | undefined) => setFilter({ owner: key, selected: selectedSessionId, session: id }),
    /** Empties the filter for `project`'s panel, so that once the pane passes to the project it lists every session's processes, whatever session is selected. */
    showAllOf: (project: string) => setFilter({ owner: ownerKey({ kind: "project", project }), selected: selectedSessionId, session: undefined }),
  };
}

const ProcessPanelContext = createContext<ReturnType<typeof useProcessPanel> | undefined>(undefined);
export const ProcessPanelProvider = ProcessPanelContext.Provider;
export const useProcessPanelContext = () => useContext(ProcessPanelContext);
