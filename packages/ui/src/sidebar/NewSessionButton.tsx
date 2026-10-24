import { Plus, Zap } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { isPrimaryModifier, useWindowShortcut } from "../useWindowShortcut";
import { matchCreateShortcut, ShortcutTooltip } from "./sessionShortcuts";
import { RowIconButton } from "./rows";
import { useNewSessionMode } from "./useNewSessionMode";

/**
 * The "+" that opens the session dialog for `target`. While the platform's shortcut modifier is
 * held it turns into a bolt named `quickLabel`, and pressing it (or pressing with the modifier even
 * when the held state was missed) calls `onQuickCreate` instead of `onOpen`. With no agent that is
 * added and installed there is nothing to create with, so it is disabled. Its quick form, when no
 * account of the agent it would create with can open a session (each is used up or turned off),
 * only looks disabled: it keeps
 * its focus, its tooltip and its description give the short reason while its name stays the action,
 * and a press calls `onQuickCreate`, which refuses with the full sentence naming the action by
 * `quickLabel`. A `target` that is not there (`undefined`) makes a press do nothing.
 */
export function NewSessionButton({
  label,
  quickLabel,
  title,
  quickTitle,
  target,
  onOpen,
  onQuickCreate,
  shortcut = false,
  ref,
}: {
  label: string;
  quickLabel: string;
  title?: React.ReactNode;
  quickTitle?: React.ReactNode;
  target: () => NewSessionRequest | undefined;
  onOpen: (target: NewSessionRequest) => void;
  onQuickCreate: (target: NewSessionRequest, action: string) => void;
  /** Registers the focus header's creation shortcuts. */
  shortcut?: boolean;
  /** The element around the button, for focus to be handed to. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const t = useT();
  const { quick, blocked, unavailable } = useNewSessionMode(target);
  useWindowShortcut((event) => {
    const action = matchCreateShortcut(event);
    return action === undefined ? undefined : { action, repeat: event.repeat };
  }, ({ action, repeat }) => {
    if (repeat || blocked) return;
    const request = target();
    if (!request) return;
    if (action === "quickCreate") onQuickCreate(request, quickLabel);
    else onOpen(request);
  }, shortcut);
  const quickUnavailable = quick && unavailable ? t("sidebar.project.openSessionQuickUsedUpShort") : undefined;
  const tooltip = quick ? (quickTitle ?? quickLabel) : (title ?? label);
  return (
    <RowIconButton
      ref={ref}
      icon={quick ? Zap : Plus}
      label={quick ? quickLabel : label}
      title={shortcut ? <ShortcutTooltip action={quick ? "quickCreate" : "newSession"}>{tooltip}</ShortcutTooltip> : tooltip}
      isDisabled={blocked}
      unavailable={quickUnavailable}
      onPress={(event) => {
        const request = target();
        if (!request) return;
        if (quick || isPrimaryModifier(event)) onQuickCreate(request, quickLabel);
        else onOpen(request);
      }}
    />
  );
}
