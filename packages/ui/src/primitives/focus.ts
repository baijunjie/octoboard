import type React from "react";
import { useLayoutEffect, useRef, useState } from "react";
import { setInteractionModality, useFocusRing, useFocusVisible, useObjectRef as useAriaObjectRef } from "react-aria";

/** Whether the current input modality is the keyboard's (react-aria's interaction modality), so focus arriving now is
 * to be shown; read in the render after focus arrives, since the key press that changes it may move focus first. */
export function useKeyboardModality(): boolean {
  return useFocusVisible().isFocusVisible;
}

/** Sets the input modality to the keyboard's, for a keyboard handler that moves focus itself and keeps the key from the
 * document's capture-phase listener that would set it: what it focuses then draws its ring (`useFocusVisibleProps`,
 * the wrappers' `data-focus-visible`). */
export function setKeyboardModality(): void {
  setInteractionModality("keyboard");
}

export type FocusVisibleProps = React.DOMAttributes<Element> & { "data-focus-visible"?: true };

/**
 * Props that mark a hand-built focusable element `data-focus-visible` while it has keyboard focus,
 * for its ring to be styled from (`ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus`; `group-ui-keyboard-focus` and
 * `has-ui-keyboard-focus` reach across it). The layer's variants in `style.css` are how the application styles any
 * element the wrappers or this hook mark, never the attribute itself.
 *
 * The state is react-aria's rather than CSS `:focus-visible`, because the app's WKWebView decides
 * that one differently from the Playwright `webkit` build: once the last focus came from a click,
 * it does not match an element focused by script, even right after a key press, so a keyboard
 * move made by script — F6 between regions, Tab inside a focus-trapped dialog — lands without a
 * ring. react-aria's state follows the input modality instead, which it reads from a capture-phase
 * listener on the document: a keyboard handler that moves focus itself and keeps the key from that
 * listener has to call `setKeyboardModality()` first, or what it focuses draws no ring.
 */
export function useFocusVisibleProps(): FocusVisibleProps {
  const { focusProps, isFocusVisible } = useFocusRing();
  return { ...focusProps, "data-focus-visible": isFocusVisible || undefined };
}

/**
 * The props of `useFocusVisibleProps` for a container whose focus is held by an input a library owns inside it
 * (xterm.js's textarea), where every key, Escape and Tab included, is typed into that input. Whether the container
 * shows focus is decided as focus lands on an element in it, from outside or from another element inside (a cover's
 * region handing focus back to the input): shown when the modality is the keyboard's then (F6 or Tab, an automatic
 * handoff that called `ringForAutomaticFocus`), not after a click. Keys typed into the focused element never turn it
 * on, as they would with react-aria's own text-input rule, since they do not move focus; a pointer press anywhere
 * turns it off, and it ends when focus leaves the container. Focus that leaves and comes back to the same element with
 * the window (the element still active meanwhile) keeps the decision made for it.
 */
export function useArrivalFocusProps(): FocusVisibleProps {
  const keyboard = useKeyboardModality();
  const [focus, setFocus] = useState<"out" | "arrived" | "shown" | "hidden">("out");
  // The element the current decision was made for.
  const decidedFor = useRef<EventTarget | null>(null);
  // Decided in the render after focus arrives, where the modality of the move that brought it is read.
  useLayoutEffect(() => {
    if (focus === "arrived") setFocus(keyboard ? "shown" : "hidden");
    else if (focus === "shown" && !keyboard) setFocus("hidden");
  }, [focus, keyboard]);
  return {
    onFocus: (event) => {
      const same = event.target === decidedFor.current;
      decidedFor.current = event.target;
      setFocus((f) => (same && f !== "out" ? f : "arrived"));
    },
    onBlur: (event) => {
      // Focus that leaves with the window stays on the element: not a departure.
      if (document.activeElement === event.target) return;
      if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
      decidedFor.current = null;
      setFocus("out");
    },
    "data-focus-visible": focus === "shown" || undefined,
  };
}

/** Whether `props`, from `useFocusVisibleProps`, say the element has keyboard focus now. */
export function hasKeyboardFocus(props: FocusVisibleProps): boolean {
  return Boolean(props["data-focus-visible"]);
}

/** An object ref that also keeps `ref`, whichever kind it is, in step with the element it holds. */
export function useObjectRef<T>(ref?: React.Ref<T>): React.RefObject<T | null> {
  return useAriaObjectRef(ref);
}

/** Marks the control a dialog's opening focus goes to, the one given `autoFocus`: react-aria leaves no trace of it in
 * the page, and for a press without a pointer it drops that focus if anything else took focus first (an alert raised as
 * the dialog opens), so the application finds it by this mark (`focusOpenDialog`). The wrappers set it from their
 * `autoFocus`; a dialog that focuses a control of its own on opening sets it on that control. */
export function openingFocus(autoFocus: boolean | undefined): { "data-opening-focus"?: "" } {
  return autoFocus ? { "data-opening-focus": "" } : {};
}

/**
 * Takes over where focus goes when a dialog or popover closes: `listener` runs once, before react-aria puts focus
 * back, and calls `preventDefault()` on the event to place focus itself. It listens in the capture phase and stops at
 * the first restore or when `signal` aborts.
 *
 * react-aria 3.52's `FocusScope` raises this cancelable `react-aria-focus-scope-restore` event on the document before it
 * applies its restore target, including the first control of a parent scope once the opener is gone. An internal event,
 * and no hook offers the choice: to check again when react-aria is upgraded.
 */
export function onFocusScopeRestore(listener: (event: Event) => void, signal: AbortSignal): void {
  document.addEventListener("react-aria-focus-scope-restore", listener, { capture: true, once: true, signal });
}
