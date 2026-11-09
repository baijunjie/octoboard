import type { Daemon } from "./connection";
import type { State } from "./state";

/** Resolves once the store's state is one `ready` accepts: at once if it is already, else on the first change that makes
 * it so. Held by the store rather than by a component, so it works for a write whose component has gone. */
export function whenState(store: Daemon["store"], ready: (state: State) => boolean): Promise<void> {
  return new Promise((resolve) => {
    if (ready(store.getState())) return resolve();
    const unsubscribe = store.subscribe((state) => {
      if (!ready(state)) return;
      unsubscribe();
      resolve();
    });
  });
}
