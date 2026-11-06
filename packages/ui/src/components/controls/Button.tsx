import { Button as HeroButton } from "@heroui/react";
import React, { useEffect } from "react";
import { useObjectRef } from "react-aria";

import { openingFocus } from "../focus/modalFocus";
import { DecorativeSpinner } from "./DecorativeSpinner";

export type ButtonProps = Omit<React.ComponentProps<typeof HeroButton>, "size"> & {
  size?: "xs" | "sm" | "md" | "lg";
};

/**
 * HeroUI's button with the shared extra-small scale for compact actions. While `isPending`, a
 * decorative spinner leads the unchanged label (it replaces the content of an icon-only button),
 * and the button reports itself busy.
 */
export function Button({ size, className, children, isPending, ref: forwarded, ...props }: ButtonProps): React.ReactElement {
  const ref = useObjectRef(forwarded);
  // React Aria's button drops `aria-busy` from props and exposes a pending button only as disabled.
  useEffect(() => {
    if (!isPending) return;
    ref.current?.setAttribute("aria-busy", "true");
    return () => ref.current?.removeAttribute("aria-busy");
  }, [isPending, ref]);
  const classes = size === "xs" ? "app-button--xs" : "";
  const spinner = <DecorativeSpinner size={size === "lg" ? "md" : "sm"} className={size === "xs" ? "size-3" : undefined} />;
  const content = (shown: React.ReactNode) => (isPending ? (props.isIconOnly ? spinner : <>{spinner}{shown}</>) : shown);
  return (
    <HeroButton
      {...props}
      ref={ref}
      isPending={isPending}
      {...openingFocus(props.autoFocus)}
      size={size === "xs" ? "sm" : size}
      data-size={size}
      className={typeof className === "function"
        ? (state) => `${classes} ${className(state)}`
        : `${classes} ${className ?? ""}`}
    >
      {typeof children === "function" ? (state) => content(children(state)) : content(children)}
    </HeroButton>
  );
}
