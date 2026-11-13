import { FolderOpen, Plus, Zap } from "lucide-react";
import React, { useRef, useState } from "react";
import { useShallow } from "zustand/react/shallow";

import { ActionMenu } from "../components/menus/ActionMenu";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { quickCreateUnavailable } from "../dialogs/openSessionRequest";
import { useT } from "../i18n/react";
import { useHost } from "../platform/react";
import type { Project } from "../protocol";
import { useDaemonStore } from "../store";
import { matchCreateShortcut, ShortcutTooltip } from "./sessionShortcuts";
import { useWindowShortcut } from "../shortcuts/useWindowShortcut";
import { sortProjects } from "../sessions/order";
import { RowIconButton } from "../components/rows/RowIconButton";
import { useNewSessionMode } from "./useNewSessionMode";
import { setKeyboardModality } from "../primitives";

/**
 * A "+" that is a menu of `projects`, for a view with no project of its own to open a session in.
 * Choosing a project opens the session dialog for `target(project)`; while the modifier is held and an
 * agent is available it turns into a bolt named `quickLabel` and choosing one calls `onQuickCreate` instead. A project whose agent has no account that can open a session (each used up, not logged in
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
  onQuickCreate: (target: NewSessionRequest) => void;
}): React.ReactElement {
  const t = useT();
  const host = useHost();
  const { quick: heldQuick, blocked } = useNewSessionMode();
  // The form the menu has as it opens holds until it closes: a shortcut chooses it (`armedRef`, taken as the menu opens),
  // and otherwise a pointer takes the one the held modifier gives. Releasing the keys must not change it, since typing
  // in the menu's filter means letting go of them.
  const [lockedForm, setLockedForm] = useState<"newSession" | "quickCreate">();
  const armedRef = useRef<"newSession" | "quickCreate">(undefined);
  const quick = lockedForm === undefined ? heldQuick : lockedForm === "quickCreate";
  const triggerRef = useRef<HTMLButtonElement>(null);
  useWindowShortcut((event) => {
    const action = matchCreateShortcut(event, host);
    return action === undefined ? undefined : { action, repeat: event.repeat };
  }, ({ action, repeat }) => {
    if (repeat || blocked || projects.length === 0) return;
    armedRef.current = action;
    setKeyboardModality();
    triggerRef.current?.focus();
    // The click opens the menu at once, which takes the armed form; one that does not open it leaves none behind.
    triggerRef.current?.click();
    armedRef.current = undefined;
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
  const disabled = projects.length === 0 || blocked;
  // The menu is gone with this branch and reports no close, so the form it locked goes too.
  if (disabled && lockedForm !== undefined) setLockedForm(undefined);
  if (disabled) return <RowIconButton icon={Plus} label={label} onPress={() => {}} isDisabled />;
  const name = quick ? quickLabel : label;
  return (
    <ActionMenu
      label={name}
      tooltip={<ShortcutTooltip action={quick ? "quickCreate" : "newSession"}>{name}</ShortcutTooltip>}
      triggerRef={triggerRef}
      onOpened={() => setLockedForm(armedRef.current ?? (heldQuick ? "quickCreate" : "newSession"))}
      onClosed={() => setLockedForm(undefined)}
      filter={{ label: t("sidebar.filter.field"), emptyMessage: t("sidebar.filter.noMatch") }}
      trigger={quick ? <Zap aria-hidden="true" className="size-4" /> : <Plus aria-hidden="true" className="size-4" />}
      items={sortProjects(projects, () => [], new Set(), new Set()).map((project) => {
        const reason = unavailable.has(project.id) ? { state: t("sidebar.project.openSessionQuickUnavailable") } : undefined;
        return {
          label: project.name,
          reason,
          icon: FolderOpen,
          onClick: () => (quick ? onQuickCreate(target(project)) : onOpen(target(project))),
        };
      })}
    />
  );
}
