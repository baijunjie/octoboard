import { Badge } from "@heroui/react";
import React from "react";

import { textDirection } from "../i18n/languages";
import { useCurrentLanguage } from "../i18n/react";

/** A decorative rail count, mirrored toward the content panel under right-to-left.
 * Counts past 99 use the localized overflow label. */
export function RailCountBadge({ count, overflowLabel, color }: {
  count: number;
  overflowLabel: string;
  color: "warning" | "success";
}): React.ReactElement {
  const rtl = textDirection(useCurrentLanguage()) === "rtl";
  const overflow = count > 99;
  return (
    <Badge
      inert
      aria-hidden="true"
      color={color}
      variant="primary"
      size="sm"
      placement={rtl ? "top-left" : "top-right"}
      // The overflow label is a number followed by a sign, which reads in that order in
      // any language; left to the surrounding direction it would come out as "+99".
      dir="ltr"
    >
      {overflow ? overflowLabel : count}
    </Badge>
  );
}
