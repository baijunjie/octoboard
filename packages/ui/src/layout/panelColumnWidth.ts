/** The usable width each card keeps before a pair stacks vertically. */
export const MIN_PANEL_COLUMN_WIDTH = 260;

export const PANEL_COLUMN_GAP = 10;
export const PANEL_ASIDE_INSET = 16;

/** One card and the pane's terminal-side and outer insets. */
export const MIN_SINGLE_COLUMN_ASIDE_WIDTH = MIN_PANEL_COLUMN_WIDTH + PANEL_ASIDE_INSET;

/** Two cards and their gap, plus the pane's terminal-side and outer insets. */
export const MIN_TWO_COLUMN_ASIDE_WIDTH = 2 * MIN_PANEL_COLUMN_WIDTH + PANEL_COLUMN_GAP + PANEL_ASIDE_INSET;
