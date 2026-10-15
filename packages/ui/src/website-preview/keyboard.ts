/** Keep touch previews browsable without opening a software keyboard, including on auto-focus. */
export function installPreviewKeyboardGuard(): () => void {
  if (!window.matchMedia("(pointer: coarse)").matches) return () => {};

  const protect = (element: Element) => {
    if (element instanceof HTMLTextAreaElement ||
      (element instanceof HTMLInputElement &&
        /^(text|search|email|url|tel|password|number)$/.test(element.type))) {
      element.readOnly = true;
      element.inputMode = "none";
    } else if (element instanceof HTMLElement && element.isContentEditable) {
      element.contentEditable = "false";
    }
  };
  const scan = (element: Element) => {
    protect(element);
    element.querySelectorAll("input, textarea, [contenteditable]").forEach(protect);
  };
  // Focus can happen synchronously after mounting, before the mutation observer runs.
  const onFocus = (event: FocusEvent) => {
    if (event.target instanceof Element) protect(event.target);
  };
  document.addEventListener("focus", onFocus, true);
  scan(document.documentElement);
  const observer = new MutationObserver((records) => {
    for (const record of records)
      for (const node of record.addedNodes)
        if (node instanceof Element) scan(node);
  });
  observer.observe(document.documentElement, { childList: true, subtree: true });
  return () => {
    observer.disconnect();
    document.removeEventListener("focus", onFocus, true);
  };
}
