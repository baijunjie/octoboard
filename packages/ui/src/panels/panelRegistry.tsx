import type { LucideIcon } from "lucide-react";
import type React from "react";

import { LazyProjectBrowser } from "../browser/LazyProjectBrowser";
import type { ViewedFile } from "../browser/TerminalFileViewer";
import type { PlainMessageKey, Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { AsideOwner } from "../layout/asideOwner";
import type { PanePeek } from "../layout/usePaneToggles";
import type { Project } from "../protocol";
import { ReportPanel } from "../report/ReportPanel";
import { ShellTerminalPanel } from "../terminal/ShellTerminalPanel";
import { PANEL_META } from "./panelMeta";
import type { PanelKind } from "./stackState";

/** What a panel's content is given: who owns the pane, which the stack follows the way the file
 * browser follows a project, and the state the panes share. */
export interface PanelContentProps {
  owner: AsideOwner;
  /** The project a project owner names, once the store has it. */
  project: Project | undefined;
  /** Whether the panel is on screen (docked or floating in), so only then does it refresh on its
   * own. */
  active: boolean;
  /** The session selected in the sidebar. */
  selectedSessionId: string | undefined;
  /** The session whose pages the Report panel shows: the console session that owns the pane, or the
   * selected project session while it has pages. */
  reportSessionId: string | undefined;
  /** The panel appears in a pane that is already on screen, as the owner changes under it; not when
   * the pane itself docks or floats in. */
  switching: boolean;
  /** Where keyboard focus goes when what held it in the panel goes away. */
  focusTerminal: () => void;
  /** The floating pane's pointer callbacks, for a panel whose content sends this document no pointer
   * events of its own; `undefined` while the pane is docked. */
  peek: Pick<PanePeek, "keep" | "leave"> | undefined;
  /** The panel is being resized every frame (a maximized card while the sidebar's dock eases): a
   * terminal keeps its size until this ends, then fits once. */
  resizing: boolean;
  /** What a key pressed inside a report page's frame asks of the window. */
  frameKeys: {
    onEscape: () => void;
    onCycleRegion: (backward: boolean) => void;
    onMoveHistory: (backward: boolean) => void;
    onMoveConsoleSession: (backward: boolean) => void;
  };
  /** Shows a project file a path in a terminal's output named, in the file viewer. `returnFocus`
   * puts focus back where the click came from once the viewer closes, and says whether it took
   * it; the agent terminal takes it when `returnFocus` is absent or returns false. */
  onViewFile?: (file: ViewedFile, returnFocus?: () => boolean) => void;
}

/** One kind of panel. The content starts with a `PanelHeader`, which supplies the stack's own
 * controls and puts the kind's title and actions in it. */
export interface PanelEntry {
  /** The kind's icon, on its toggle in the top bar. */
  icon: LucideIcon;
  /** What the kind is called in a sentence, as in a handle's name. */
  name: PlainMessageKey;
  /** The labels of the kind's toggle in the top bar, for what pressing it does. */
  toggle: { show: PlainMessageKey; hide: PlainMessageKey };
  /** The panel's accessible name as a region. */
  label: (t: Translate, project: Project | undefined) => string;
  /** Whether its header offers Maximize panel, which makes the docked card cover the agent terminal's
   * area (the sidebar and the other panels stay as they are) until it is restored. */
  maximizable?: boolean;
  Content: React.ComponentType<PanelContentProps>;
}

function FilesContent({ project, active, selectedSessionId, switching, focusTerminal }: PanelContentProps): React.ReactElement | null {
  return project ? <LazyProjectBrowser project={project} active={active} selectedSessionId={selectedSessionId} switching={switching} focusTerminal={focusTerminal} /> : null;
}

function TerminalContent({ project, switching, resizing, onViewFile }: PanelContentProps): React.ReactElement | null {
  return project ? <ShellTerminalPanel project={project} switching={switching} resizing={resizing} onViewFile={onViewFile} /> : null;
}

function ReportContent({ reportSessionId, peek, frameKeys }: PanelContentProps): React.ReactElement | null {
  // Keyed by the session, so another session's panel starts afresh, on its newest page.
  return reportSessionId ? <ReportPanel key={reportSessionId} sessionId={reportSessionId} peek={peek} {...frameKeys} /> : null;
}

/** Every kind of panel the stack can hold: a later kind is one more entry here and in
 * `PANEL_KINDS`. */
export const PANEL_ENTRIES: Record<PanelKind, PanelEntry> = {
  files: {
    ...PANEL_META.files,
    toggle: { show: "titleBar.files.show", hide: "titleBar.files.hide" },
    label: (t, project) => t("browser.label", { project: isolate(project?.name ?? "") }),
    Content: FilesContent,
  },
  terminal: {
    ...PANEL_META.terminal,
    toggle: { show: "titleBar.terminal.show", hide: "titleBar.terminal.hide" },
    label: (t, project) => t("panel.terminal.label", { project: isolate(project?.name ?? "") }),
    maximizable: true,
    Content: TerminalContent,
  },
  report: {
    ...PANEL_META.report,
    toggle: { show: "titleBar.report.show", hide: "titleBar.report.hide" },
    label: (t) => t("panel.report.name"),
    Content: ReportContent,
  },
};
