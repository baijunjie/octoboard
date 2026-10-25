import { Surface } from "@heroui/react";
import type { ReactElement } from "react";

import { useT } from "../i18n/react";
import { CodeBlock } from "./CodeBlock";
import { FadedScroller, useFadeRegion } from "./scrollFade";

/** Raw diagnostics sit on a HeroUI `Surface` of their own (the secondary variant), not directly on the dialog, alert or
 * popover around them, and retain their line layout; overflowing lines remain reachable by keyboard. The scrolling
 * region is named "Error details", or by the element `labelledBy` where several blocks stand together. The text takes
 * the tone of a `CodeToneContext` around it, as an error screen sets. A block that may be taller than the room it has
 * (a crash's stack) passes `tall`: it scrolls vertically too, up to 40% of the viewport, and fades at the edge it
 * hides the rest behind. A `wrap`ped block is for a message in prose-like lines (a daemon's startup failure): it
 * breaks long lines instead of scrolling, so it has no overflow to name as a region or to fade. */
export function DiagnosticBlock({ children, labelledBy, tall = false, wrap = false }: { children: string; labelledBy?: string; tall?: boolean; wrap?: boolean }): ReactElement {
  const t = useT();
  const region = useFadeRegion(labelledBy ? { labelledBy } : { label: t("error.diagnostics") });
  if (wrap) {
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
        className={`min-w-0 max-w-full rounded-md ${tall ? "max-h-[40vh] overflow-auto" : "overflow-x-auto overflow-y-hidden"}`}
      >
        <CodeBlock dir="ltr" wrap={false} className="[&>.app-code]:rounded-none [&>.app-code]:bg-transparent">{children}</CodeBlock>
      </FadedScroller>
    </Surface>
  );
}
