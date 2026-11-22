/** What can cover the session terminal, each with the window's own way of clearing it. */
export interface TerminalCovers {
  /** Clears the panels over the terminal's area that the layout says cover it (`usePaneLayout`'s `revealArchive`). */
  uncoverPanels: () => void;
  /** Whether the archive view is open, and the way the user closes it. */
  archiveOpen: boolean;
  closeArchive: () => void;
  /** Whether the Remote devices or Scheduled tasks page shows, and the way of leaving it that goes through the guard of
   * changes not saved (`leavePage`): with changes held it asks first, and a page the user keeps stays. */
  pageShown: boolean;
  leavePage: (go: () => void) => void;
  /** Runs inside the leave, so only once the page is left: where focus is then handed to the terminal. */
  pageLeft: () => void;
}

/** Brings the session terminal back into view, for a quote written into its input: each cover goes through its owner's
 * own action. A page kept at the question of changes not saved stays over the terminal, and focus stays with it. */
export function uncoverTerminal({ uncoverPanels, archiveOpen, closeArchive, pageShown, leavePage, pageLeft }: TerminalCovers): void {
  uncoverPanels();
  if (archiveOpen) closeArchive();
  if (pageShown) leavePage(pageLeft);
}
