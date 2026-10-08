import type { PlatformAdapter } from "./platform";

/** Where the webview's own context menu stays: it is how text is copied and pasted there. */
const NATIVE_MENU = 'input, textarea, [contenteditable]:not([contenteditable="false"]), .xterm';

/**
 * Suppresses the webview's right-click menu, whose Back / Reload / Inspect entries make the window
 * look like a web page. Only in the desktop shell: a browser tab keeps its own menu wherever the UI
 * does not offer one itself (the rows and headers that carry an action menu open it on right-click).
 */
export function suppressNativeContextMenu(platform: PlatformAdapter): void {
  if (platform.kind !== "tauri") return;
  document.addEventListener("contextmenu", (event) => {
    if (event.target instanceof Element && event.target.closest(NATIVE_MENU)) return;
    event.preventDefault();
  });
}
