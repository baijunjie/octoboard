import { Typography } from "@heroui/react";
import React from "react";
import { TextContext } from "react-aria-components";

import type { DataAttributes } from "./props";

/** HeroUI's code text: preformatted code in its typography. It is never a slot of a component around it (an alert's
 * description, an item's label), so the text context such a component provides is reset around it. */
export function Code({ dir, className, children, ...data }: DataAttributes & {
  dir?: "auto" | "ltr" | "rtl";
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <TextContext.Provider value={{}}>
      <Typography.Code {...data} dir={dir} className={className}>{children}</Typography.Code>
    </TextContext.Provider>
  );
}
