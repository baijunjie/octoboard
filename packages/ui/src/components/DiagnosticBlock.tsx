import type { ReactElement } from "react";

import { useT } from "../i18n/react";
import { CodeBlock } from "./CodeBlock";
import { FadedScroller, useFadeRegion } from "./scrollFade";

/** Raw diagnostics retain their line layout; overflowing lines remain reachable by keyboard. The scrolling region
 * is named "Error details", or by the element `labelledBy` where several blocks stand together. */
export function DiagnosticBlock({ children, labelledBy }: { children: string; labelledBy?: string }): ReactElement {
  const t = useT();
  const region = useFadeRegion(labelledBy ? { labelledBy } : { label: t("error.diagnostics") });
  return (
    <div className={`min-w-0 max-w-full rounded-md bg-default ${region.focused ? "ring-2 ring-app-focus" : ""}`}>
      <FadedScroller dir="ltr" orientation="horizontal" region={region} className="min-w-0 max-w-full overflow-x-auto overflow-y-hidden rounded-md">
        <CodeBlock dir="ltr" wrap={false} className="[&>.app-code]:rounded-none [&>.app-code]:bg-transparent">{children}</CodeBlock>
      </FadedScroller>
    </div>
  );
}
