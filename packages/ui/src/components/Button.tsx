import { Button as HeroButton } from "@heroui/react";
import React from "react";

import { openingFocus } from "./regionFocus";

export type ButtonProps = Omit<React.ComponentProps<typeof HeroButton>, "size"> & {
  size?: "xs" | "sm" | "md" | "lg";
};

/** HeroUI's button with the shared extra-small scale for compact actions. */
export function Button({ size, className, ...props }: ButtonProps): React.ReactElement {
  const classes = size === "xs" ? "app-button--xs" : "";
  return (
    <HeroButton
      {...props}
      {...openingFocus(props.autoFocus)}
      size={size === "xs" ? "sm" : size}
      data-size={size}
      className={typeof className === "function"
        ? (state) => `${classes} ${className(state)}`
        : `${classes} ${className ?? ""}`}
    />
  );
}
