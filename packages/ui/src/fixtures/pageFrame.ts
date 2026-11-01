// The page set-up the gallery and the website preview share before they load the app, kept apart from the modules
// that read stored preferences as they are evaluated: the page applies the theme before the app loads, which is why
// this file imports nothing from the app.

/** What `index.html`'s bootstrap script does for the real page: the theme class and attribute on the root element, so
 * the first paint, and the error screen outside `ThemeProvider`, are themed. Any class the page's HTML gave it is
 * replaced. */
export function applyDocumentTheme(doc: Document, theme: "light" | "dark"): void {
  doc.documentElement.className = theme;
  doc.documentElement.dataset.theme = theme;
}

/** Resolves two frames after the call, or after `capMs` if the page draws none (a background tab does not), so that
 * what the last step set going (a render, a transition's first frames) has been drawn when the page says it is ready.
 * The cap's timer is cleared when the frames win, so nothing is left pending. */
export function afterFrames(capMs: number): Promise<void> {
  return new Promise<void>((resolve) => {
    const cap = setTimeout(resolve, capMs);
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        clearTimeout(cap);
        resolve();
      }),
    );
  });
}
