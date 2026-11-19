/**
 * A way for a control outside the window that owns a dialog (an error alert, rendered beside it) to open that
 * dialog. A module-level slot rather than a context because the control is not inside the window: the window
 * registers how it opens the dialog, and `open` calls whatever is registered, doing nothing while none is.
 *
 * Registrations stack: `open` calls the latest one still registered, and one going away hands back to the one before
 * it. Two owners can be registered at once for a moment (one mounting as another unmounts, a panel shown in two panes
 * while it moves), and the one left must still be reachable.
 */
export function createLauncher<A extends unknown[]>(): {
  /** Registers the window's way of opening the dialog, and returns what unregisters it. */
  register: (open: (...args: A) => void) => () => void;
  /** Opens the dialog; does nothing while no window has registered. */
  open: (...args: A) => void;
} {
  // Each registration in a box of its own, so the same function registered twice unregisters once each time.
  const launchers: { open: (...args: A) => void }[] = [];
  return {
    register(open) {
      const entry = { open };
      launchers.push(entry);
      return () => {
        const at = launchers.indexOf(entry);
        if (at !== -1) launchers.splice(at, 1);
      };
    },
    open: (...args) => launchers.at(-1)?.open(...args),
  };
}
