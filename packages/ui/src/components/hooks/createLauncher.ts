/**
 * A way for a control outside the window that owns a dialog (an error alert, rendered beside it) to open that
 * dialog. A module-level slot rather than a context because the control is not inside the window: the window
 * registers how it opens the dialog, and `open` calls whatever is registered, doing nothing while none is.
 */
export function createLauncher<A extends unknown[]>(): {
  /** Registers the window's way of opening the dialog, and returns what unregisters it. */
  register: (open: (...args: A) => void) => () => void;
  /** Opens the dialog; does nothing while no window has registered. */
  open: (...args: A) => void;
} {
  let launcher: ((...args: A) => void) | undefined;
  return {
    register(open) {
      launcher = open;
      return () => {
        if (launcher === open) launcher = undefined;
      };
    },
    open: (...args) => launcher?.(...args),
  };
}
