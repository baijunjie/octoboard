import React from "react";

import { AsidePane, type AsideLayout } from "../layout/AsidePane";
import type { AsideOwner } from "../layout/asideOwner";
import type { Project } from "../protocol";
import type { PanelContentProps } from "./panelRegistry";
import { PanelStack } from "./PanelStack";
import type { PanelKind } from "./stackState";

/**
 * The right pane: the pane's frame (`layout/AsidePane.tsx`) around the stack of its owner's panels.
 * The frame paints nothing, as the cards carry their own surface and edge. It is shared by every
 * owner; the panels' contents are keyed by owner, so nothing read for one project or console
 * session can land in another's.
 */
export function RightPane({
  owner,
  project,
  layout,
  active,
  selectedSessionId,
  focusTerminal,
  frameKeys,
  peekKind,
  onDismiss,
}: {
  owner: AsideOwner;
  project: Project | undefined;
  layout: AsideLayout;
  /** Whether the pane is on screen, docked or floating in. */
  active: boolean;
  selectedSessionId: string | undefined;
  focusTerminal: () => void;
  frameKeys: PanelContentProps["frameKeys"];
  /** The kind whose toggle the pointer is on, shown while the pane floats in with no panel open. */
  peekKind: PanelKind | undefined;
  /** Closing a panel that was only floating in dismisses the floating pane. */
  onDismiss: () => void;
}): React.ReactElement {
  return (
    <AsidePane layout={layout}>
      <PanelStack
        owner={owner}
        project={project}
        active={active}
        selectedSessionId={selectedSessionId}
        focusTerminal={focusTerminal}
        peek={layout.peek}
        peekKind={peekKind}
        frameKeys={frameKeys}
        onDismiss={onDismiss}
      />
    </AsidePane>
  );
}
