import type React from "react";
import { useFocusRing } from "react-aria";

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
 * listener has to call `setInteractionModality("keyboard")` first, or what it focuses draws no ring.
 */
export function useFocusVisibleProps(): FocusVisibleProps {
  const { focusProps, isFocusVisible } = useFocusRing();
  return { ...focusProps, "data-focus-visible": isFocusVisible || undefined };
}
