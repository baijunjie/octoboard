/** The kinds of panel the right pane can stack, in the order they are stacked top to bottom and the
 * order F6 visits them. A later kind (a browser) is one more entry here and one in
 * `panels/panelRegistry.tsx`. */
export const PANEL_KINDS = ["report", "terminal", "files", "processes"] as const;
export type PanelKind = (typeof PANEL_KINDS)[number];
