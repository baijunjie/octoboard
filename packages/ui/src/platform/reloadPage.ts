/** Set once the page has started to reload: the navigation is asynchronous, and what still runs before the page unloads
 * must not do what the reloaded page will do (say the update notice, which it would then never show). */
let reloading = false;

/** Reloads the page, which a daemon that updated itself serves anew; kept apart so a test can stand in for it. */
export function reloadPage(): void {
  reloading = true;
  window.location.reload();
}

/** Whether `reloadPage` was called: the page is about to unload. */
export function isReloading(): boolean {
  return reloading;
}
