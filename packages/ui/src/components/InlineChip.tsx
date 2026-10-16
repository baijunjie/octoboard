import { Chip } from "@heroui/react";
import React from "react";

import { useCodeColor, type CodeColor } from "./codeTone";

export type InlineChipSize = "xs" | "sm" | "md" | "lg";
export type InlineChipProps = Omit<React.ComponentProps<typeof Chip<"span">>, "size" | "color" | "children"> & {
  children?: React.ReactNode;
  size?: InlineChipSize;
  color?: CodeColor | "accent";
  monospace?: boolean;
  wrap?: boolean;
  startContent?: React.ReactNode;
};

/** One chip scale for prose, labels and metadata. `xs` keeps dense rows one 16px line tall. */
export function InlineChip({
  children,
  size = "sm",
  color = "default",
  variant = "soft",
  monospace = false,
  wrap = false,
  startContent,
  dir = monospace ? "ltr" : undefined,
  className = "",
  ...props
}: InlineChipProps): React.ReactElement {
  const resolvedColor = useCodeColor(color === "accent" ? "primary" : color);
  const nativeColor = resolvedColor === "primary" ? "accent" : resolvedColor === "secondary" ? "default" : resolvedColor;
  const text = wrap && typeof children === "string" ? children.split("/").map((part, i) => (
    <React.Fragment key={i}>{i > 0 && <>/<wbr /></>}{part}</React.Fragment>
  )) : children;
  return (
    <Chip<"span">
      {...props}
      size={size === "xs" ? "sm" : size}
      color={nativeColor}
      variant={variant}
      dir={dir}
      data-inline-chip=""
      data-size={size}
      data-color={resolvedColor}
      className={`inline-chip ${size === "xs" ? "inline-chip--xs" : ""} ${wrap ? "inline-chip--wrap" : ""} ${monospace ? "font-mono select-text" : ""} ${resolvedColor === "secondary" ? "inline-chip--secondary" : ""} ${className}`}
    >
      {startContent}
      <Chip.Label>
        {monospace ? <code dir={dir} data-color={resolvedColor} className="inline-chip__code select-text">{text}</code> : text}
      </Chip.Label>
    </Chip>
  );
}
