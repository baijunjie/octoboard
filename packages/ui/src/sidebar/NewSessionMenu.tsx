import { FolderOpen, Plus, Zap } from "lucide-react";
import React from "react";
import { useShallow } from "zustand/react/shallow";

import { ActionMenu } from "../components/ActionMenu";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { quickCreateUsedUp } from "../dialogs/openSessionRequest";
import { useT } from "../i18n/react";
import type { Project } from "../protocol";
import { useDaemonStore } from "../store";
import { sortProjects } from "./order";
import { RowIconButton } from "./rows";
import { useNewSessionMode } from "./useNewSessionMode";

/**
 * A "+" that is a menu of `projects`, for a view with no project of its own to open a session in.
 * Choosing a project opens the session dialog for `target(project)`; while the modifier is held and an
 * agent is available it turns into a bolt named `quickLabel` and choosing one calls `onQuickCreate` (with
 * `quickLabel`) instead. A project whose agent has every account used up is disabled then, with why under
 * its name and in its accessible name. With no project, or no agent that is added and installed, it is a
 * disabled "+".
 */
export function NewSessionMenu({
  label,
  quickLabel,
  projects,
  target,
  onOpen,
  onQuickCreate,
}: {
  label: string;
  quickLabel: string;
  projects: Project[];
  target: (project: Project) => NewSessionRequest;
  onOpen: (target: NewSessionRequest) => void;
  onQuickCreate: (target: NewSessionRequest, action: string) => void;
}): React.ReactElement {
  const t = useT();
  const { quick, blocked } = useNewSessionMode();
  // In the quick form, the projects whose agent has no account left to run a session on cannot be chosen.
  // The time is read as the store is: the modifier is held only for a moment.
  const usedUp = useDaemonStore(useShallow((s) => {
    const ids = new Set<string>();
    if (quick) {
      for (const project of projects) {
        if (quickCreateUsedUp(target(project), s, Date.now())) ids.add(project.id);
      }
    }
    return ids;
  }));
  if (projects.length === 0 || blocked) return <RowIconButton icon={Plus} label={label} onPress={() => {}} isDisabled />;
  const name = quick ? quickLabel : label;
  return (
    <ActionMenu
      label={name}
      tooltip={name}
      trigger={quick ? <Zap aria-hidden="true" className="size-4" /> : <Plus aria-hidden="true" className="size-4" />}
      items={sortProjects(projects, () => []).map((project) => {
        const reason = usedUp.has(project.id) ? { state: t("sidebar.project.openSessionQuickUsedUpShort") } : undefined;
        return {
          label: project.name,
          reason,
          icon: FolderOpen,
          onClick: () => (quick ? onQuickCreate(target(project), quickLabel) : onOpen(target(project))),
        };
      })}
    />
  );
}
