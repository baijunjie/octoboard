import { Maximize2, Minimize2, X } from "lucide-react";
import React, { createContext, useContext } from "react";

import { Button } from "./Button";
import { FadeOverflow } from "./FadeOverflow";
import { ROW_ICON_BUTTON_CLASS, RowIconGroup } from "./RowControls";
import { FadedScroller } from "./scrollFade";
import { TitledControl } from "./TitledControl";
import { useT } from "../i18n/react";

/** What a panel's card tells its header about the panel's place in the stack. */
export interface PanelFrame {
  /** Whether the header offers maximize / restore: a maximizable kind (the Terminal panel), docked. */
  canMaximize: boolean;
  maximized: boolean;
  onToggleMaximized: () => void;
  onClose: () => void;
}

const PanelFrameContext = createContext<PanelFrame | undefined>(undefined);
export const PanelFrameProvider = PanelFrameContext.Provider;

/**
 * The header every panel's content starts with, laid out as the header of the sidebar's floating
 * card is (the same height, `--pane-header-height-floating`, inset, gap, title size and weight, and
 * icon controls): the title, whatever the kind puts beside it (`children`, such as a tab list) and
 * its own actions at the end, then the stack's maximize / restore, for a kind that offers it, and
 * close, which every kind has. When the pane is too narrow for all of it the children give way (a tab list scrolls),
 * never the title or the controls at the end. A header outside a stack (a test)
 * has neither of the stack's controls. `prioritizeChildren` keeps a mode switch whole instead:
 * the title gives way first, then the kind's actions scroll, with close always outside the clip.
 * `hideTitleWhenNarrow` keeps the heading accessible but only draws it in a docked-width window
 * when the header's content box has at least 352px for the title, tabs and actions.
 */
export function PanelHeader({
  title,
  actions,
  children,
  prioritizeChildren = false,
  hideTitleWhenNarrow = false,
}: {
  title: string;
  /** The kind's own icon controls. */
  actions?: React.ReactNode;
  children?: React.ReactNode;
  /** Keep the mode switch and close visible, giving up the title before the scrollable actions. */
  prioritizeChildren?: boolean;
  /** Hide the visual title below docked width or 352px of header content, retaining its heading. */
  hideTitleWhenNarrow?: boolean;
}): React.ReactElement {
  const t = useT();
  const frame = useContext(PanelFrameContext);
  const heading = (
    <h2 className={`${hideTitleWhenNarrow ? "sr-only docked:@min-[352px]/panel-header:not-sr-only docked:@min-[352px]/panel-header:flex" : "flex"} min-w-0 max-w-40 ${prioritizeChildren ? "overflow-hidden" : "shrink-0"} text-base font-semibold`}>
      <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={title}>{title}</FadeOverflow>
    </h2>
  );
  return (
    <div className={`${hideTitleWhenNarrow ? "@container/panel-header " : ""}${prioritizeChildren ? "grid min-w-0 grid-cols-[minmax(min-content,1fr)_minmax(0,max-content)_max-content]" : "flex"} h-(--pane-header-height-floating) shrink-0 items-center gap-2 groove-b px-(--pane-inset)`}>
      {prioritizeChildren ? (
        <div className="grid grid-cols-[minmax(0,max-content)_max-content] items-center gap-2">
          {heading}
          {children}
        </div>
      ) : <>{heading}{children}</>}
      {prioritizeChildren && (
        <FadedScroller orientation="horizontal" className="min-w-0 p-1.5 -m-1.5 has-[[data-focus-visible=true]]:mask-none">
          <RowIconGroup className="w-max">{actions}</RowIconGroup>
        </FadedScroller>
      )}
      <RowIconGroup className={prioritizeChildren ? "" : "ms-auto"}>
        {!prioritizeChildren && actions}
        {frame?.canMaximize && (
          <TitledControl title={t(frame.maximized ? "panel.restore" : "panel.maximize")}>
            <Button isIconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t(frame.maximized ? "panel.restore" : "panel.maximize")} preventFocusOnPress onPress={frame.onToggleMaximized}>
              {frame.maximized ? <Minimize2 aria-hidden="true" className="size-4" /> : <Maximize2 aria-hidden="true" className="size-4" />}
            </Button>
          </TitledControl>
        )}
        {frame && (
          <TitledControl title={t("panel.close")}>
            <Button isIconOnly size="xs" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t("panel.close")} preventFocusOnPress onPress={frame.onClose}>
              <X aria-hidden="true" className="size-4" />
            </Button>
          </TitledControl>
        )}
      </RowIconGroup>
    </div>
  );
}
