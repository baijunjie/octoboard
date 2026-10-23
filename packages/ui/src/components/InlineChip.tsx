import { Chip } from "@heroui/react";
import React from "react";

import { useCodeColor, type CodeColor } from "./codeTone";

export type InlineChipSize = "xs" | "sm" | "md" | "lg";
export type InlineChipProps = Omit<React.ComponentProps<typeof Chip<"span">>, "size" | "color" | "children"> & {
  children?: React.ReactNode;
  size?: InlineChipSize;
  color?: CodeColor;
  monospace?: boolean;
  wrap?: boolean;
  /** Moves to the next line as one box rather than splitting where the line ends, so no empty
   * start of it is left behind; text longer than the line still wraps inside it (with `wrap`). */
  unbroken?: boolean;
  startContent?: React.ReactNode;
};

/** A reference mixed into running copy. It always takes the font of the text it sits in; the sizes differ only in
 * padding, and `xs`, the default, is safe in every line (see `docs/ui-accessibility.md`'s "A chip's size follows the
 * line it sits in" section). */
export function InlineChip({
  children,
  size = "xs",
  color = "default",
  variant = "soft",
  monospace = false,
  wrap = false,
  unbroken = false,
  startContent,
  dir = monospace ? "ltr" : undefined,
  className = "",
  ...props
}: InlineChipProps): React.ReactElement {
  const resolvedColor = useCodeColor(color);
  const nativeColor = resolvedColor === "help" ? "default" : resolvedColor;
  const text = wrap && typeof children === "string" ? children.split("/").map((part, i) => (
    <React.Fragment key={i}>{i > 0 && <>/<wbr /></>}{part}</React.Fragment>
  )) : children;
  return (
    <Chip<"span">
      {...props}
      size="sm"
      color={nativeColor}
      variant={variant}
      dir={dir}
      data-inline-chip=""
      data-size={size}
      data-color={resolvedColor}
      className={`inline-chip ${wrap ? "inline-chip--wrap" : ""} ${unbroken ? "inline-chip--unbroken" : ""} ${monospace ? "select-text" : ""} ${className}`}
    >
      {startContent}
      <Chip.Label>
        {monospace ? <code dir={dir} data-color={resolvedColor} className="inline-chip__code select-text">{text}</code> : text}
      </Chip.Label>
    </Chip>
  );
}
