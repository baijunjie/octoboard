import { Surface, Typography } from "@heroui/react";
import React from "react";
import { TextContext } from "react-aria-components";

import { useCodeColor, type CodeColor } from "../../i18n/codeTone";

/** The classes of the secondary `Surface` a flush code block sits on: rounded corners, never wider than its container. */
export const CODE_SURFACE_CLASS = "min-w-0 max-w-full rounded-md";

/** Raw text such as a diagnostic or a licence; long traces can leave wrapping to their surrounding horizontal
 * scroller. `flush` drops the block's own rounded corners and fill, for a block that sits on a surface of its own. */
export function CodeBlock({
  children,
  color,
  dir = "auto",
  wrap = true,
  flush = false,
}: {
  children: string;
  color?: CodeColor;
  dir?: "auto" | "ltr" | "rtl";
  wrap?: boolean;
  flush?: boolean;
}): React.ReactElement {
  const resolvedColor = useCodeColor(color);
  return (
    <pre dir={dir} className={`app-code-block m-0 ${wrap ? "min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]" : "min-w-full w-max whitespace-pre"}${flush ? " [&>.app-code]:rounded-none [&>.app-code]:bg-transparent" : ""}`}>
      <TextContext.Provider value={{}}>
        <Typography.Code dir={dir} data-color={resolvedColor} className="app-code block w-full p-3 text-start select-text">
          {children}
        </Typography.Code>
      </TextContext.Provider>
    </pre>
  );
}

/** A code block that wraps, on a rounded secondary surface of its own: a left-to-right text read whole, such as a
 * diagnostic or a licence. */
export function SurfaceCodeBlock({ children }: { children: string }): React.ReactElement {
  return (
    <Surface variant="secondary" className={CODE_SURFACE_CLASS}>
      <CodeBlock dir="ltr" flush>{children}</CodeBlock>
    </Surface>
  );
}
