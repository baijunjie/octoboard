/** How often the browser's lists on screen (directories, branches, changes) are read again while the browser is on
 * screen and the window is in front, so what an agent adds, removes or commits shows up without asking. */
export const AUTO_REFRESH_MS = 10_000;

/** How old the last read of a list on screen must be for coming back to the window, or to a pane that was hidden, to
 * read it again at once rather than at the next interval. */
export const AUTO_REFRESH_CATCH_UP_MS = AUTO_REFRESH_MS / 2;
