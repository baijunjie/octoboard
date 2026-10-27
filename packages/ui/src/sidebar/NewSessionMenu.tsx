import { FolderOpen, Plus, Zap } from "lucide-react";
import React, { useRef, useState } from "react";
import { flushSync } from "react-dom";
import { setInteractionModality } from "react-aria";
import { useShallow } from "zustand/react/shallow";

import { ActionMenu } from "../components/ActionMenu";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { quickCreateUnavailable } from "../dialogs/openSessionRequest";
import { useT } from "../i18n/react";
import type { Project } from "../protocol";
import { useDaemonStore } from "../store";
import { matchCreateShortcut, ShortcutTooltip } from "./sessionShortcuts";
import { useWindowShortcut } from "../useWindowShortcut";
import { sortProjects } from "./order";
import { RowIconButton } from "./rows";
import { useNewSessionMode } from "./useNewSessionMode";

/**
 * A "+" that is a menu of `projects`, for a view with no project of its own to open a session in.
 * Choosing a project opens the session dialog for `target(project)`; while the modifier is held and an
 * agent is available it turns into a bolt named `quickLabel` and choosing one calls `onQuickCreate` (with
 * `quickLabel`) instead. A project whose agent has no account that can open a session (each used up, not logged in
 * or turned off) is disabled then, with why under its name and in its accessible name. With no project, or
 * no agent that is added and installed, it is a disabled "+".
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
  const { quick: heldQuick, blocked } = useNewSessionMode();
  // A shortcut chooses the action before the project picker opens; releasing its keys must not
  // change that choice. Pointer-opened menus keep following the held modifier.
  const [shortcutMode, setShortcutMode] = useState<"newSession" | "quickCreate">();
  const quick = shortcutMode === undefined ? heldQuick : shortcutMode === "quickCreate";
  const triggerRef = useRef<HTMLButtonElement>(null);
  useWindowShortcut((event) => {
    const action = matchCreateShortcut(event);
    return action === undefined ? undefined : { action, repeat: event.repeat };
  }, ({ action, repeat }) => {
    if (repeat || blocked || projects.length === 0) return;
    flushSync(() => setShortcutMode(action));
    setInteractionModality("keyboard");
    triggerRef.current?.focus();
    triggerRef.current?.click();
  });
  // In the quick form, the projects whose agent has no account that can open a session (each used up, not logged in
  // or turned off) cannot be chosen. The time is read as the store is: the modifier is held only for a
  // moment.
  const unavailable = useDaemonStore(useShallow((s) => {
    const ids = new Set<string>();
    if (quick) {
      for (const project of projects) {
        if (quickCreateUnavailable(target(project), s, Date.now())) ids.add(project.id);
      }
    }
    return ids;
  }));
  if (projects.length === 0 || blocked) return <RowIconButton icon={Plus} label={label} onPress={() => {}} isDisabled />;
  const name = quick ? quickLabel : label;
  return (
    <ActionMenu
      label={name}
      tooltip={<ShortcutTooltip action={quick ? "quickCreate" : "newSession"}>{name}</ShortcutTooltip>}
      triggerRef={triggerRef}
      onClosed={() => setShortcutMode(undefined)}
      trigger={quick ? <Zap aria-hidden="true" className="size-4" /> : <Plus aria-hidden="true" className="size-4" />}
      items={sortProjects(projects, () => []).map((project) => {
        const reason = unavailable.has(project.id) ? { state: t("sidebar.project.openSessionQuickUsedUpShort") } : undefined;
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
