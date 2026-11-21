import React from "react";

import { Chip, ChipLabel } from "../primitives";
import { useCodeColor, type CodeColor } from "./codeTone";

export type InlineReferenceSize = "xs" | "sm" | "md" | "lg";
export type InlineReferenceProps = Omit<React.ComponentProps<typeof Chip>, "size" | "color" | "children"> & {
  children?: React.ReactNode;
  size?: InlineReferenceSize;
  color?: CodeColor;
  monospace?: boolean;
  wrap?: boolean;
  /** Moves to the next line as one box rather than splitting where the line ends, so no empty
   * start of it is left behind; text longer than the line still wraps inside it (with `wrap`). */
  unbroken?: boolean;
  startContent?: React.ReactNode;
};

/** A reference mixed into running copy, where prose would use quotation marks (`docs/conventions/ui-standard.md`,
 * "Badge, reference or plain name"). It always takes the font of the text it sits in — size, weight, line height and
 * family, a technical literal keeping its monospace `code` child at the same size — and sits inline on that text's
 * baseline. The sizes differ only in padding, the tint drawn around the text:
 *
 * | Size | Padding (block, inline) | Drawn height |
 * | --- | --- | --- |
 * | `xs` | 0, 4px | the text's content area, about 1.2 times its font size |
 * | `sm` | 1px, 6px | content area + 2px |
 * | `md` | 2px, 8px | content area + 4px |
 * | `lg` | 3px, 10px | content area + 6px |
 *
 * The drawn height stays below the copy's line height, so chips on wrapped lines never touch and no line is stretched:
 * 12px text on 16px lines (field errors, secondary descriptions, tooltips) fits only `xs`; 14px on 20px (body copy,
 * labels, Alert descriptions) up to `sm`; 16px on 24px (dialog headings) up to `md`; 14px on 24px (Alert titles) up to
 * `lg`. `xs`, the default, is safe in every copy style, and a path's length, font or position never picks a size. An
 * `unbroken` chip, an `inline-block` as tall as its line box, tightens its own line height to the content area so it
 * does not touch the next line's. Colours come from the tint tokens shared with `StatusBadge`; inside an Alert the
 * nearest status context (`CodeToneContext`) wins over an explicit colour. */
export function InlineReference({
  children,
  size = "xs",
  color = "default",
  monospace = false,
  wrap = false,
  unbroken = false,
  startContent,
  dir = monospace ? "ltr" : undefined,
  className = "",
  ...props
}: InlineReferenceProps): React.ReactElement {
  const resolvedColor = useCodeColor(color);
  const nativeColor = resolvedColor === "help" ? "default" : resolvedColor;
  // A wrapping reference may break after each path separator, a Windows path's `\` as much as `/`.
  const text = wrap && typeof children === "string" ? children.split(/(?<=[/\\])/).map((part, i) => (
    <React.Fragment key={i}>{i > 0 && <wbr />}{part}</React.Fragment>
  )) : children;
  return (
    <Chip
      {...props}
      size="sm"
      color={nativeColor}
      dir={dir}
      data-inline-reference=""
      data-size={size}
      data-color={resolvedColor}
      className={`inline-reference ${wrap ? "inline-reference--wrap" : ""} ${unbroken ? "inline-reference--unbroken" : ""} ${monospace ? "select-text" : ""} ${className}`}
    >
      {startContent}
      <ChipLabel>
        {monospace ? <code dir={dir} data-color={resolvedColor} className="inline-reference__code select-text">{text}</code> : text}
      </ChipLabel>
    </Chip>
  );
}
