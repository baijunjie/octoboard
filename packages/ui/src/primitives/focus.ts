import type React from "react";
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

type FocusVisibleProps = React.DOMAttributes<Element> & { "data-focus-visible"?: true };

/**
 * Props that mark a hand-built focusable element `data-focus-visible` while it has keyboard focus,
 * for its ring to be styled from (`data-focus-visible:ring-2 data-focus-visible:ring-focus`).
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
