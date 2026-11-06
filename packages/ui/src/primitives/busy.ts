import { createContext, useContext } from "react";

/** Whether the modal around a control is busy: a request it started is out and cannot be taken back, so every control
 * in it is disabled, except the action that is pending. `Modal` provides it (its `busy` prop). */
export const BusyContext = createContext(false);

/** Whether the control is in a busy modal. For one that is not drawn by a wrapper (a link, a raw element), which then
 * renders unavailable; every interactive wrapper reads it through `useDisabled`. */
export function useBusy(): boolean {
  return useContext(BusyContext);
}

/** The disabled state of a control: its own `disabled`, and always disabled in a busy modal. `exempt` is for the one
 * control that stays operable there: a pending button, which holds the focus the busy modal hands on to it. Left
 * `undefined` while nothing disables the control, so the library's context can still fill it in. */
export function useDisabled(disabled?: boolean, exempt = false): boolean | undefined {
  const busy = useBusy();
  return busy && !exempt ? true : disabled;
}
