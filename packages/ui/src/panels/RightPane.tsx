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
  onViewFile,
  onDismiss,
  maximizeArea,
  areaEasing,
  float,
  floating,
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
  /** Shows a project file a path in a terminal's output named, in the file viewer. */
  onViewFile?: PanelContentProps["onViewFile"];
  /** Closing a panel that was only floating in dismisses the floating pane. */
  onDismiss: () => void;
  /** The agent terminal's area, which a maximized panel covers. */
  maximizeArea: React.RefObject<HTMLElement | null>;
  /** The sidebar's dock is easing, which resizes the area frame by frame. */
  areaEasing?: boolean;
  /** The hover reveal of a panel that is not open, while the pane's column is folded away
   * (`layout.folded`, its one open panel maximized): the panel floats in a pane of its own, the
   * folded one holding the maximized card, which has to stay where it is. */
  float?: AsideLayout["peek"];
  /** In a window too narrow to dock the pane, the one panel floating over the agent terminal
   * (`PanelStack`'s `floating`). */
  floating?: React.ComponentProps<typeof PanelStack>["floating"];
}): React.ReactElement {
  const shared = { owner, project, selectedSessionId, focusTerminal, peekKind, frameKeys, onViewFile, onDismiss, maximizeArea, areaEasing };
  return (
    <>
      <AsidePane layout={layout}>
        {(peek) => <PanelStack {...shared} active={active} peek={peek} floating={floating} />}
      </AsidePane>
      {float && (
        <AsidePane layout={{ width: layout.width, peek: float }}>
          <PanelStack {...shared} active={float.active} peek={float} floatOnly />
        </AsidePane>
      )}
    </>
  );
}
