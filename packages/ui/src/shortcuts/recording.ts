/** What a recorder does with a key press: `consumed` keeps it from everything else on the page. */
type Recorder = (event: KeyboardEvent) => "consumed" | "passed";

let active: Recorder | undefined;

// Installed as this module is evaluated, which is before any component adds a listener of its own
// to the window, so while a shortcut is being recorded this one sees every key first and is the
// only one that does (`stopImmediatePropagation`): the window's shortcuts, the dialog's Escape and
// the terminal all stay out of it.
if (typeof window !== "undefined") {
  window.addEventListener(
    "keydown",
    (event) => {
      if (!active || event.isComposing) return;
      if (active(event) === "consumed") {
        event.preventDefault();
        event.stopImmediatePropagation();
      }
    },
    { capture: true },
  );
}

/** Hands every key press to `recorder` until the returned function is called. One recording at a time: a newer replaces an older. */
export function recordKeys(recorder: Recorder): () => void {
  active = recorder;
  return () => {
    if (active === recorder) active = undefined;
  };
}
