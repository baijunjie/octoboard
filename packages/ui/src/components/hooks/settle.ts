// Sits outside the components and hooks that call it because the React Compiler skips a function holding a `try` with
// a `finally`.

/** Awaits `action`, handing a rejection to `onError` instead of letting it escape, and calls `onSettled` after
 * either outcome. */
export async function settle(action: () => void | Promise<void>, onError: (err: unknown) => void, onSettled: () => void): Promise<void> {
  try {
    await action();
  } catch (err) {
    onError(err);
  } finally {
    onSettled();
  }
}
