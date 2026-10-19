import { Button } from "@heroui/react";
import { Maximize2, Minimize2, X } from "lucide-react";
import React, { createContext, useContext } from "react";

import { FadeOverflow } from "./FadeOverflow";
import { ROW_ICON_BUTTON_CLASS, RowIconGroup } from "./RowControls";
import { TitledControl } from "./TitledControl";
import { useT } from "../i18n/react";

/** What a panel's card tells its header about the panel's place in the stack. */
export interface PanelFrame {
  /** Whether there is another panel to hide, which is what maximizing is for. */
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
 * its own actions at the end, then the stack's maximize / restore and close, which are the same for
 * every kind. When the pane is too narrow for all of it the title gives way first, then the
 * children (a tab list scrolls), never the controls at the end. A header outside a stack (a test)
 * has neither of the stack's controls.
 */
export function PanelHeader({
  title,
  actions,
  children,
}: {
  title: string;
  /** The kind's own icon controls. */
  actions?: React.ReactNode;
  children?: React.ReactNode;
}): React.ReactElement {
  const t = useT();
  const frame = useContext(PanelFrameContext);
  return (
    <div className="flex h-(--pane-header-height-floating) shrink-0 items-center gap-2 groove-b px-(--pane-inset)">
      <h2 className="flex min-w-0 max-w-40 shrink-[3] text-base font-semibold">
        <FadeOverflow as="span" dir="auto" className="min-w-0 flex-1" titleWhenClipped={title}>{title}</FadeOverflow>
      </h2>
      {children}
      <RowIconGroup className="ms-auto">
        {actions}
        {frame?.canMaximize && (
          <TitledControl title={t(frame.maximized ? "panel.restore" : "panel.maximize")}>
            <Button isIconOnly size="sm" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t(frame.maximized ? "panel.restore" : "panel.maximize")} preventFocusOnPress onPress={frame.onToggleMaximized}>
              {frame.maximized ? <Minimize2 aria-hidden="true" className="size-4" /> : <Maximize2 aria-hidden="true" className="size-4" />}
            </Button>
          </TitledControl>
        )}
        {frame && (
          <TitledControl title={t("panel.close")}>
            <Button isIconOnly size="sm" variant="ghost" className={ROW_ICON_BUTTON_CLASS} aria-label={t("panel.close")} preventFocusOnPress onPress={frame.onClose}>
              <X aria-hidden="true" className="size-4" />
            </Button>
          </TitledControl>
        )}
      </RowIconGroup>
    </div>
  );
}
