// A failure of the gallery's own machinery — a scenario step that did not run, a fixture that
// cannot be answered faithfully — shown over the page.

/** The one container every banner line is appended to, made on the first failure. */
let container: HTMLElement | undefined;
const shown = new Set<string>();

/**
 * Puts `message` over the page in red, and logs it with `cause` where there is one, so a stack is
 * still in the console. A scenario whose own data or steps failed shows something other than what
 * it says, and whoever opened it is looking at the page rather than at a test run, so the failure
 * has to be on the page too. An identical message is shown once.
 */
export function galleryFailure(message: string, cause?: unknown): void {
  if (cause === undefined) console.error(message);
  else console.error(message, cause);
  if (typeof document === "undefined" || shown.has(message)) return;
  shown.add(message);
  if (!container) {
    container = document.createElement("div");
    container.dataset.galleryFailure = "true";
    container.style.cssText =
      "position:fixed;inset-inline:0;bottom:0;z-index:9999;padding:8px;background:#b91c1c;color:#fff;font:12px monospace;white-space:pre-wrap";
    document.body.append(container);
  }
  const line = document.createElement("div");
  line.textContent = message;
  container.append(line);
}
