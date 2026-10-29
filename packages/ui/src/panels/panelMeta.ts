import { FileText, FolderClosed, SquareTerminal, type LucideIcon } from "lucide-react";

import type { PlainMessageKey } from "../i18n/catalog";
import type { PanelKind } from "./stackState";

/** What a kind of panel is called and drawn as, apart from its content, so a menu can offer a panel
 * without importing the panels themselves (`PANEL_ENTRIES`, which builds on this). */
export const PANEL_META: Record<PanelKind, { icon: LucideIcon; name: PlainMessageKey }> = {
  files: { icon: FolderClosed, name: "panel.files.name" },
  terminal: { icon: SquareTerminal, name: "panel.terminal.name" },
  report: { icon: FileText, name: "panel.report.name" },
};
