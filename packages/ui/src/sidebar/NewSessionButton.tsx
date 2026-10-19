import { Plus, Zap } from "lucide-react";
import React from "react";

import type { NewSessionRequest } from "../dialogs/dialogRequest";
import { isPrimaryModifier } from "../useWindowShortcut";
import { RowIconButton } from "./rows";
import { useNewSessionMode } from "./useNewSessionMode";

/**
 * The "+" that opens the session dialog for `target`. While the platform's shortcut modifier is
 * held it turns into a bolt named `quickLabel`, and pressing it (or pressing with the modifier even
 * when the held state was missed) calls `onQuickCreate` instead of `onOpen`. With no agent that is
 * added and installed there is nothing to create with, so it is disabled. A `target` that is not
 * there (`undefined`) makes a press do nothing.
 */
export function NewSessionButton({
  label,
  quickLabel,
  target,
  onOpen,
  onQuickCreate,
  ref,
}: {
  label: string;
  quickLabel: string;
  target: () => NewSessionRequest | undefined;
  onOpen: (target: NewSessionRequest) => void;
  onQuickCreate: (target: NewSessionRequest) => void;
  /** The element around the button, for focus to be handed to. */
  ref?: React.Ref<HTMLDivElement>;
}): React.ReactElement {
  const { quick, blocked } = useNewSessionMode();
  return (
    <RowIconButton
      ref={ref}
      icon={quick ? Zap : Plus}
      label={quick ? quickLabel : label}
      isDisabled={blocked}
      onPress={(event) => {
        const request = target();
        if (!request) return;
        if (quick || isPrimaryModifier(event)) onQuickCreate(request);
        else onOpen(request);
      }}
    />
  );
}
