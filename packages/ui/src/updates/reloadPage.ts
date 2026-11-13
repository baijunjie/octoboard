/** Reloads the page, which a daemon that updated itself serves anew; kept apart so a test can stand in for it. */
export function reloadPage(): void {
  window.location.reload();
}
