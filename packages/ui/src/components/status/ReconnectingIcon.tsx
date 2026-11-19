import { LoaderCircle } from "lucide-react";
import React from "react";

const SIZES = {
  compact: "size-3.5",
  normal: "size-4",
  cover: "size-6",
};

export function ReconnectingIcon({
  size = "normal",
  inBanner = false,
}: {
  size?: keyof typeof SIZES;
  inBanner?: boolean;
}): React.ReactElement {
  return (
    <LoaderCircle
      className={`${SIZES[size]} shrink-0 motion-safe:animate-spin-slow ${inBanner ? "text-inherit" : "text-[var(--tint-warning-fg)]"}`}
    />
  );
}
