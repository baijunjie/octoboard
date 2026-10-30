import { FileText, FolderClosed, SquareActivity, SquareTerminal, type LucideIcon } from "lucide-react";

import type { PlainMessageKey } from "../i18n/catalog";
import type { PanelKind } from "./stackState";

/** What a kind of panel is called and drawn as, apart from its content, so a menu can offer a panel
 * without importing the panels themselves (`PANEL_ENTRIES`, which builds on this). */
export const PANEL_META: Record<PanelKind, { icon: LucideIcon; name: PlainMessageKey }> = {
  report: { icon: FileText, name: "panel.report.name" },
  files: { icon: FolderClosed, name: "panel.files.name" },
  processes: { icon: SquareActivity, name: "panel.processes.name" },
  terminal: { icon: SquareTerminal, name: "panel.terminal.name" },
};
