import { Button as HeroButton } from "@heroui/react";
import { Button as AriaButton } from "react-aria-components";
import { buttonVariants } from "@heroui/styles";
import React, { useEffect, useId } from "react";

import { useDisabled } from "./busy";
import { useFieldClear } from "./Field";
import { openingFocus, useObjectRef } from "./focus";
import { Spinner } from "./Spinner";
import type { DataAttributes } from "./props";

/** HeroUI's button variants, by HeroUI's own names. */
export type ButtonVariant = "primary" | "secondary" | "tertiary" | "outline" | "ghost" | "danger" | "danger-soft";

/** HeroUI's button sizes, and `xs`: the project's extra-small size for compact actions (`docs/ui-standard.md`, "Pointer
 * target size"), 24px high, a 24px square when icon-only, with 12px text and 14px icons (`.app-button--xs`,
 * `style.css`). */
export type ButtonSize = "xs" | "sm" | "md" | "lg";

/** A press of a button: what pressed it, so a caller can tell a keyboard activation from a pointer's. */
export interface ButtonPress {
  pointerType: "mouse" | "pen" | "touch" | "keyboard" | "virtual";
  /** The modifier keys held during the press, for a button that does something else with one held. */
  metaKey: boolean;
  ctrlKey: boolean;
}

/** Focus leaving a button: where it went, `null` when it went nowhere (the button turning disabled under focus, the
 * window losing focus). */
export interface ButtonBlur {
  movedTo: EventTarget | null;
}

/** The class names that draw an element as a button of `variant` and `size`, for an element that is not a button but
 * looks like one (a link, a disclosure's trigger). */
export function buttonClassName({ variant, size }: { variant?: ButtonVariant; size?: ButtonSize } = {}): string {
  const look = buttonVariants({ variant, size: size === "xs" ? "sm" : size });
  return size === "xs" ? `${look} ${XS_CLASS}` : look;
}

/** The extra-small size on top of HeroUI's small one; the unlayered `.app-button--xs` rules extend HeroUI's scale. */
const XS_CLASS = "app-button--xs";

/** The look of an unavailable button over HeroUI's disabled one, which turns pointer events off for `aria-disabled`:
 * they are turned back on, for the tooltip with the reason to open and a press to reach `onPress`, and the hover and
 * pressed fills stay the resting one and the press shrink is left off, as nothing is done. A fill of the caller's own
 * classes (a row icon's `hover:` tint) is the caller's to leave off. */
const UNAVAILABLE_CLASS = "pointer-events-auto [--button-bg-hover:var(--button-bg)] [--button-bg-pressed:var(--button-bg)] ui-pressed:[transform:none] active:[transform:none]";

/** What a plain button and a toggle button share: its name and description, and how it takes focus. */
export interface ButtonBaseProps extends DataAttributes {
  id?: string;
  /** The accessible name, for a button whose content does not give one (an icon-only button). */
  label?: string;
  /** The ids of the elements that name it, in place of `label`. */
  labelledBy?: string;
  /** The ids of the elements that describe it; `""` keeps a tooltip that repeats the name from describing it. */
  describedBy?: string;
  /** For a button that shows or hides something: whether it is shown, as `aria-expanded`. */
  expanded?: boolean;
  /** The id of what the button shows, hides or opens, as `aria-controls`. */
  controls?: string;
  size?: ButtonSize;
  /** Draws the button as a square holding only an icon; `label` names it. */
  iconOnly?: boolean;
  disabled?: boolean;
  /** Whether a press moves focus to the button; `false` leaves focus where it was (on the terminal, an editor). */
  focusOnPress?: boolean;
  /** Whether Tab reaches the button; `false` keeps it out of the tab order (a control another keyboard walk reaches). */
  tabStop?: boolean;
  /** Focuses the button as it mounts, and marks it as where its dialog's opening focus goes (`openingFocus`). */
  autoFocus?: boolean;
  /** Called as a mouse pointer enters or leaves the button; touch and pen are ignored. */
  onMouseHoverChange?: (hovered: boolean) => void;
  onFocus?: () => void;
  onBlur?: (blur: ButtonBlur) => void;
  ref?: React.Ref<HTMLButtonElement>;
  className?: string;
  children?: React.ReactNode;
}

/** The press of a button as the project's `ButtonPress`. */
function toButtonPress(event: { pointerType: ButtonPress["pointerType"]; metaKey: boolean; ctrlKey: boolean }): ButtonPress {
  return { pointerType: event.pointerType, metaKey: event.metaKey, ctrlKey: event.ctrlKey };
}

/** The props of a button's element that `ButtonBaseProps` maps onto, shared by both buttons. */
export function baseButtonProps({
  label, labelledBy, describedBy, expanded, controls, disabled, focusOnPress, tabStop, autoFocus,
  onMouseHoverChange, onFocus, onBlur, iconOnly, ...rest
}: Omit<ButtonBaseProps, "size" | "className" | "children">) {
  return {
    ...rest,
    ...openingFocus(autoFocus),
    "aria-label": label,
    "aria-labelledby": labelledBy,
    "aria-describedby": describedBy,
    "aria-expanded": expanded,
    "aria-controls": controls,
    isDisabled: disabled,
    isIconOnly: iconOnly,
    // Passed only when the caller turns them off: a field's own button (`SearchField`'s clear button) is taken out of the
    // tab order and kept from taking focus through the button context, which an explicit `false` would override.
    preventFocusOnPress: focusOnPress === false || undefined,
    excludeFromTabOrder: tabStop === false || undefined,
    autoFocus,
    onHoverStart: onMouseHoverChange && ((event: { pointerType: string }) => event.pointerType === "mouse" && onMouseHoverChange(true)),
    onHoverEnd: onMouseHoverChange && ((event: { pointerType: string }) => event.pointerType === "mouse" && onMouseHoverChange(false)),
    onFocus: onFocus && (() => onFocus()),
    onBlur: onBlur && ((event: { relatedTarget: EventTarget | null }) => onBlur({ movedTo: event.relatedTarget })),
  };
}

/** HeroUI's button, in its `variant` (`primary` by default) and `size` (HeroUI's `md` by default, or the project's
 * `xs`). While `pending`, a decorative spinner leads the unchanged content (it replaces the content of an icon-only
 * button), and the button reports itself busy (`aria-busy`). In a busy `Modal` it is disabled, unless it is `pending`:
 * the pending button is the one control left, and takes the focus the others give up (see `busy.ts`). */
export function Button({ variant, size, type = "button", pending, unavailable, current, hasPopup, onPress, className, children, ref: forwarded, ...rest }: ButtonBaseProps & {
  variant?: ButtonVariant;
  /** `submit` submits the form around it. */
  type?: "button" | "submit";
  /** Shows a spinner before the unchanged content and keeps the button where it is, unavailable and busy, while what it
   * started runs. */
  pending?: boolean;
  /** Why the button cannot act now, which makes it unavailable rather than disabled (see "Buttons" in
   * `docs/ui-standard.md`): it stays focusable and pressable, reads as `aria-disabled` and in HeroUI's disabled style
   * with neither hover fill nor press shrink, and has the reason as its description, from a visually hidden copy inside
   * it, in place of `describedBy`; its name stays the action. A press still reaches `onPress`, which answers it, and
   * the caller shows the reason as its tooltip. */
  unavailable?: string;
  /** Marks the button as the current item of a set (`aria-current`). */
  current?: boolean;
  /** Overrides whether the button says it opens a popup (`aria-haspopup`), for a trigger whose press does not always
   * open one. */
  hasPopup?: boolean;
  onPress?: (press: ButtonPress) => void;
}): React.ReactElement {
  // Placed as a field's clear button (`SearchField`, `SelectClearButton`), it takes what makes it the field's.
  const { className: placement, ...fieldClear } = useFieldClear() ?? {};
  const disabled = useDisabled(rest.disabled, pending);
  const ref = useObjectRef(forwarded);
  // React Aria's button drops `aria-busy` from props and exposes a pending button only as disabled.
  useEffect(() => {
    if (!pending) return;
    const element = ref.current;
    element?.setAttribute("aria-busy", "true");
    return () => element?.removeAttribute("aria-busy");
  }, [pending, ref]);
  const spinner = pending && <Spinner color="current" size={size === "lg" ? "md" : "sm"} className={size === "xs" ? "size-3" : undefined} />;
  const reasonId = useId();
  const reason = unavailable !== undefined && <span id={reasonId} className="sr-only">{unavailable}</span>;
  return (
    <HeroButton
      {...baseButtonProps({ ...rest, disabled, describedBy: unavailable === undefined ? rest.describedBy : reasonId })}
      ref={ref}
      {...fieldClear}
      type={type}
      variant={variant}
      size={size === "xs" ? "sm" : size}
      data-size={size}
      isPending={pending}
      aria-disabled={unavailable !== undefined || undefined}
      aria-current={current ? "true" : undefined}
      aria-haspopup={hasPopup}
      onPress={onPress && ((event) => onPress(toButtonPress(event)))}
      className={[size === "xs" && XS_CLASS, unavailable !== undefined && UNAVAILABLE_CLASS, className, placement].filter(Boolean).join(" ") || undefined}
    >
      {reason}
      {spinner && rest.iconOnly ? spinner : <>{spinner}{children}</>}
    </HeroButton>
  );
}

export type ButtonProps = React.ComponentProps<typeof Button>;

/** A button the project draws itself, with none of HeroUI's look: for a control that is a whole band or row (a section
 * heading that folds its section) rather than a button among others. It sets `data-focus-visible`, `data-pressed` and
 * `data-hovered` on its element as React Aria does, for the caller's classes to draw those states. */
export function BareButton({ onPress, className, ...rest }: Omit<ButtonBaseProps, "iconOnly" | "size"> & {
  iconOnly?: never;
  size?: never;
  onPress?: (press: ButtonPress) => void;
}): React.ReactElement {
  const disabled = useDisabled(rest.disabled);
  return (
    <AriaButton
      {...baseButtonProps({ ...rest, disabled })}
      onPress={onPress && ((event) => onPress(toButtonPress(event)))}
      className={className}
    />
  );
}
