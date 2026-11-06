import { Chip as HeroChip } from "@heroui/react";
import React from "react";

import type { DataAttributes } from "./props";

export type ChipColor = "default" | "accent" | "success" | "warning" | "danger";

/** HeroUI's soft chip, as a `span`: a small rounded mark in running content. Its content goes in directly, with no
 * padding of the chip's own text label; `ChipLabel` adds that label where it is wanted. The other props, a `ref` among
 * them, go to the chip's `span`. */
export function Chip({ size, color, children, ...props }: Omit<React.ComponentProps<"span">, "color"> & DataAttributes & {
  size: "sm" | "md" | "lg";
  color: ChipColor;
  children?: React.ReactNode;
}): React.ReactElement {
  return (
    <HeroChip<"span"> {...props} size={size} color={color} variant="soft">
      {/* A fragment rather than bare text: HeroUI wraps text given directly in its own label. */}
      <>{children}</>
    </HeroChip>
  );
}

/** HeroUI's chip label: the chip's text, padded as the chip's own label is. */
export function ChipLabel({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroChip.Label>{children}</HeroChip.Label>;
}
