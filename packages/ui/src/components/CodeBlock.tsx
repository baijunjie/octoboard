import { Typography } from "@heroui/react";
import React from "react";
import { TextContext } from "react-aria-components";

import { useCodeColor, type CodeColor } from "./codeTone";

/** Raw diagnostic text; long traces can leave wrapping to their surrounding horizontal scroller. */
export function CodeBlock({
  children,
  color,
  dir = "auto",
  wrap = true,
  className = "",
}: {
  children: string;
  color?: CodeColor;
  dir?: "auto" | "ltr" | "rtl";
  wrap?: boolean;
  className?: string;
}): React.ReactElement {
  const resolvedColor = useCodeColor(color);
  return (
    <pre dir={dir} className={`app-code-block m-0 ${wrap ? "min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]" : "min-w-full w-max whitespace-pre"} ${className}`}>
      <TextContext.Provider value={{}}>
        <Typography.Code dir={dir} data-color={resolvedColor} className="app-code block w-full p-3 text-start select-text">
          {children}
        </Typography.Code>
      </TextContext.Provider>
    </pre>
  );
}
