import { createContext, useContext, useState } from "react";
import { useStore } from "zustand/react";

import { sameValue } from "../values/sameValue";
import type { Daemon } from "./connection";
import { createStateStore, type State } from "./state";

const DaemonContext = createContext<Daemon | undefined>(undefined);

export const DaemonProvider = DaemonContext.Provider;

const EMPTY_STORE = createStateStore({});

/** `useDaemonStore` for what also renders outside a `DaemonProvider` (the error alerts, which can come up before the
 * daemon is reached): with no daemon, the selector reads an empty state that never changes. */
export function useDaemonStoreOrEmpty<T>(selector: (state: State) => T): T {
  const daemon = useContext(DaemonContext);
  return useStore(daemon?.store ?? EMPTY_STORE, selector);
}

/** The requests and actions; they never change, so using this never re-renders. */
export function useDaemon(): Daemon {
  const daemon = useContext(DaemonContext);
  if (!daemon) throw new Error("useDaemon() called outside a DaemonProvider");
  return daemon;
}

/** Reads a slice of the state. A selector that builds a new object or array each call has to be
 * wrapped in zustand's `useShallow`, or the component re-renders on every change. */
export function useDaemonStore<T>(selector: (state: State) => T): T {
  return useStore(useDaemon().store, selector);
}

/** A selector's result that is the same value as the last one it gave, whatever new objects it was
 * built from, is that last result. */
function keepingSameValue<T>(): (next: T) => T {
  let last: { value: T } | undefined;
  return (next) => {
    if (last && sameValue(last.value, next)) return last.value;
    last = { value: next };
    return next;
  };
}

/** `useDaemonStore` for a selector that derives something new from the state each time — an object
 * with others inside it, say, which `useShallow` would take as changed every time: the component
 * renders again only when the result differs in what it holds. */
export function useDaemonStoreEqual<T>(selector: (state: State) => T): T {
  const [same] = useState(keepingSameValue<T>);
  return useDaemonStore((state) => same(selector(state)));
}
