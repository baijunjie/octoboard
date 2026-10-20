import { Plus, Zap } from "lucide-react";
import React from "react";

import { useT } from "../i18n/react";
import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { isPrimaryModifier } from "../useWindowShortcut";
import { RowIconButton } from "./rows";
import { useNewSessionMode } from "./useNewSessionMode";

/**
 * The "+" that opens the session dialog for `target`. While the platform's shortcut modifier is
 * held it turns into a bolt named `quickLabel`, and pressing it (or pressing with the modifier even
 * when the held state was missed) calls `onQuickCreate` instead of `onOpen`. With no agent that is
 * added and installed there is nothing to create with, so it is disabled. Its quick form, when every
 * account of the agent it would create with has used up its quota, only looks disabled: it keeps
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
  ref,
}: {
  label: string;
  quickLabel: string;
  title?: React.ReactNode;
  quickTitle?: React.ReactNode;
  target: () => NewSessionRequest | undefined;
  onOpen: (target: NewSessionRequest) => void;
  onQuickCreate: (target: NewSessionRequest, action: string) => void;
  /** The element around the button, for focus to be handed to. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const t = useT();
  const { quick, blocked, usedUp } = useNewSessionMode(target);
  const quickUnavailable = quick && usedUp ? t("sidebar.project.openSessionQuickUsedUpShort") : undefined;
  return (
    <RowIconButton
      ref={ref}
      icon={quick ? Zap : Plus}
      label={quick ? quickLabel : label}
      title={quick ? quickTitle : title}
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
