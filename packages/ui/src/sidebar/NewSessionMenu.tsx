import { FolderOpen, Plus, Zap } from "lucide-react";
import React from "react";

import { ActionMenu } from "../components/ActionMenu";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import type { Project } from "../protocol";
import { sortProjects } from "./order";
import { RowIconButton } from "./rows";
import { useNewSessionMode } from "./useNewSessionMode";

/**
 * A "+" that is a menu of `projects`, for a view with no project of its own to open a session in.
 * Choosing a project opens the session dialog for `target(project)`; while the modifier is held
 * and an agent is available it turns into a bolt named `quickLabel` and choosing one calls
 * `onQuickCreate` instead. With no project it is a disabled "+".
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
  onQuickCreate: (target: NewSessionRequest) => void;
}): React.ReactElement {
  const { quick } = useNewSessionMode();
  if (projects.length === 0) return <RowIconButton icon={Plus} label={label} onPress={() => {}} isDisabled />;
  const name = quick ? quickLabel : label;
  return (
    <ActionMenu
      label={name}
      tooltip={name}
      trigger={quick ? <Zap aria-hidden="true" className="size-4" /> : <Plus aria-hidden="true" className="size-4" />}
      items={sortProjects(projects, () => []).map((project) => ({
        label: project.name,
        icon: FolderOpen,
        onClick: () => (quick ? onQuickCreate : onOpen)(target(project)),
      }))}
    />
  );
}
