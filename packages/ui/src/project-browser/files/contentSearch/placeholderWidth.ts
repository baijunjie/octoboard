import type React from "react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useLatest } from "../../../components/hooks/useLatest";

/** The canvas text is measured on, made the first time it is asked for; null where the page cannot draw text. */
let measuring: CanvasRenderingContext2D | null | undefined;
function measuringContext(): CanvasRenderingContext2D | null {
  if (measuring === undefined) measuring = document.createElement("canvas").getContext("2d");
  return measuring;
}

/** The least width of the row that holds the field `input` for the field to show all of its placeholder `text`, in
 * pixels: the text in the input's font, with the input's padding and border, and what else the row holds beside the
 * input (the toggle and the gap); undefined until the input is laid out or where text cannot be measured.
 *
 * It follows layout as `docs/conventions/ui-code.md` ("Performance in the app's WebKit") has it: measured after every
 * render, in a layout effect so the first paint has it, and whenever the input resizes (the field grows taller below the
 * tablet width, and the toggle beside it with it) or the interface's font finishes loading; state is written only when
 * the width moved. */
export function usePlaceholderWidth(input: React.RefObject<HTMLInputElement | null>, text: string): number | undefined {
  const [width, setWidth] = useState<number>();
  const observed = useRef<{ element: HTMLInputElement; observer: ResizeObserver }>(undefined);
  const measure = useLatest(() => {
    const element = input.current;
    const context = measuringContext();
    if (!element || !context) return;
    const style = getComputedStyle(element);
    context.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
    const sides = ["paddingLeft", "paddingRight", "borderLeftWidth", "borderRightWidth"] as const;
    const beside = element.parentElement ? element.parentElement.getBoundingClientRect().width - element.getBoundingClientRect().width : 0;
    const next = Math.ceil(context.measureText(text).width + sides.reduce((sum, side) => sum + (parseFloat(style[side]) || 0), 0) + beside);
    setWidth((current) => (current === next ? current : next));
  });
  useLayoutEffect(() => {
    // The observer follows the input React renders now, which it may have replaced in place.
    const element = input.current;
    if (observed.current?.element !== element) {
      observed.current?.observer.disconnect();
      observed.current = undefined;
      if (element) {
        const observer = new ResizeObserver(() => measure.current());
        observer.observe(element);
        observed.current = { element, observer };
      }
    }
    measure.current();
  });
  useEffect(() => {
    let live = true;
    void document.fonts?.ready.then(() => { if (live) measure.current(); });
    return () => {
      live = false;
      observed.current?.observer.disconnect();
      observed.current = undefined;
    };
  }, []);
  return width;
}
