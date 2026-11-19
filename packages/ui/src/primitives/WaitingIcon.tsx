import { LoaderCircle } from "lucide-react";
import React from "react";

const SIZES = {
  compact: "size-3.5",
  normal: "size-4",
  cover: "size-6",
};

/** Decorative activity glyph. The surrounding view names the state or announces its changes. */
export function WaitingIcon({ size = "normal", className = "" }: {
  size?: keyof typeof SIZES;
  className?: string;
}): React.ReactElement {
  return <LoaderCircle className={`${SIZES[size]} shrink-0 motion-safe:animate-spin ${className}`} />;
}
