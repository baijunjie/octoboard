import { Surface } from "@heroui/react";
import { createContext, useContext, type ReactElement } from "react";

import { useT } from "../i18n/react";
import { CodeBlock } from "./CodeBlock";
import { FadedScroller, useFadeRegion } from "./scrollFade";

/** The id of the summary of the disclosure a block stands in, which names the block's region unless it is given its own
 * `labelledBy`: a block a caller passes as the content of an "Error details" disclosure is labelled by the disclosure
 * without knowing its id. */
export const DiagnosticLabelContext = createContext<string | undefined>(undefined);

/** Raw diagnostics sit on a HeroUI `Surface` of their own (the secondary variant), not directly on the dialog, alert or
 * popover around them, and retain their line layout; overflowing lines remain reachable by keyboard. The scrolling
 * region is named "Error details", or by the element `labelledBy` where several blocks stand together (or by the
 * summary of the disclosure around it, see `DiagnosticLabelContext`). The text takes the tone of a `CodeToneContext`
 * around it, as an Alert sets. A block that may be taller than the room it has (a crash's stack) passes `tall`: it
 * scrolls vertically too, up to 40% of the viewport, and fades at the edge it hides the rest behind. A `wrap`ped block
 * breaks long lines instead of scrolling sideways (a daemon's startup failure, a crash's stack), so on its own it has no
 * overflow to name as a region or to fade; with `tall` too (in a popover or a dialog, whose height is bounded) it
 * scrolls only vertically. A `tall` block that does not wrap scrolls both ways but is a region and fades only for its
 * height, so a block whose lines can be wider than its box wraps. */
export function DiagnosticBlock({ children, labelledBy, tall = false, wrap = false }: { children: string; labelledBy?: string; tall?: boolean; wrap?: boolean }): ReactElement {
  const t = useT();
  const disclosure = useContext(DiagnosticLabelContext);
  const labelId = labelledBy ?? disclosure;
  const region = useFadeRegion(labelId ? { labelledBy: labelId } : { label: t("error.diagnostics") });
  if (wrap && !tall) {
    return (
      <Surface variant="secondary" className="min-w-0 max-w-full rounded-md">
        <CodeBlock dir="ltr" className="[&>.app-code]:rounded-none [&>.app-code]:bg-transparent">{children}</CodeBlock>
      </Surface>
    );
  }
  return (
    <Surface variant="secondary" className={`min-w-0 max-w-full rounded-md ${region.focused ? "ring-2 ring-focus" : ""}`}>
      <FadedScroller
        dir="ltr"
        orientation={tall ? "vertical" : "horizontal"}
        region={region}
        className={`min-w-0 max-w-full rounded-md ${tall ? `max-h-[40vh] ${wrap ? "overflow-x-hidden overflow-y-auto" : "overflow-auto"}` : "overflow-x-auto overflow-y-hidden"}`}
      >
        <CodeBlock dir="ltr" wrap={wrap} className="[&>.app-code]:rounded-none [&>.app-code]:bg-transparent">{children}</CodeBlock>
      </FadedScroller>
    </Surface>
  );
}
