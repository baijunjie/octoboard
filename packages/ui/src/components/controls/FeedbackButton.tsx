import { Check, type LucideIcon } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { Button } from "../../primitives";
import { StatusAnnouncer } from "../announce/StatusAnnouncer";
import { TitledControl } from "../hint/TitledControl";

/** How long the done state outlasts the tooltip's close, so the tooltip's exit animation (100 ms)
 * is over before the text and icon go back, and the user never sees them flip while it fades. */
export const RESET_AFTER_TOOLTIP_CLOSE_MS = 150;

/** How long the done state lasts when no tooltip is open to close it (a press that did not show
 * one, such as a tap). */
export const DONE_HOLD_MS = 2000;

/** The done state of a button that confirms what it did where the user is looking: set by `markDone`, ended by the
 * tooltip closing (`onTooltipOpenChange`) or, where no tooltip is open, by `DONE_HOLD_MS`. `everDone` says it has been
 * set, so the idle glyph fades back in afterwards but not on mount. */
export function useDoneFeedback(): { done: boolean; everDone: boolean; markDone: () => void; onTooltipOpenChange: (isOpen: boolean) => void } {
  const [done, setDone] = useState(false);
  const [everDone, setEverDone] = useState(false);
  const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const tooltipOpen = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    // Set again on every mount: StrictMode mounts, cleans up and mounts once more.
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(resetTimer.current);
    };
  }, []);
  return {
    done,
    everDone,
    markDone: () => {
      if (!mounted.current) return;
      clearTimeout(resetTimer.current);
      setDone(true);
      setEverDone(true);
      if (!tooltipOpen.current) resetTimer.current = setTimeout(() => setDone(false), DONE_HOLD_MS);
    },
    onTooltipOpenChange: (isOpen) => {
      tooltipOpen.current = isOpen;
      clearTimeout(resetTimer.current);
      if (!isOpen) resetTimer.current = setTimeout(() => setDone(false), RESET_AFTER_TOOLTIP_CLOSE_MS);
    },
  };
}

/**
 * The button `CopyButton` and `PasteButton` share: an icon button whose tooltip (`idleText`) turns into `doneText` and
 * stays open through the press, whose icon changes from `Icon` to a check that fades in (and back, once done), and
 * whose `doneText` is also announced through a status region, since the accessible name of a control that keeps focus
 * is not reliably re-read. It is icon-only unless given `children`, drawn as a label before the icon. The other props
 * are the `Button`'s.
 */
export function FeedbackButton({
  feedback,
  idleText,
  doneText,
  icon: Icon,
  onPress,
  buttonRef,
  children,
  ...buttonProps
}: {
  feedback: ReturnType<typeof useDoneFeedback>;
  idleText: string;
  doneText: string;
  icon: LucideIcon;
  onPress: () => void;
  buttonRef?: React.Ref<HTMLButtonElement>;
  children?: React.ReactNode;
} & Omit<React.ComponentProps<typeof Button>, "children" | "onPress" | "iconOnly" | "ref">): React.ReactElement {
  const tooltipText = feedback.done ? doneText : idleText;
  const iconOnly = children === undefined || children === null || children === false;
  return (
    <>
      <TitledControl title={tooltipText} keepOpenOnPress onOpenChange={feedback.onTooltipOpenChange}>
        <Button {...buttonProps} ref={buttonRef} iconOnly={iconOnly} label={iconOnly ? tooltipText : buttonProps.label} onPress={onPress}>
          {children}
          {feedback.done
            ? <Check key="done" className="size-4 motion-safe:animate-copy-icon-in" />
            : <Icon key="idle" className={feedback.everDone ? "size-4 motion-safe:animate-copy-icon-in" : "size-4"} />}
        </Button>
      </TitledControl>
      <StatusAnnouncer text={feedback.done ? doneText : undefined} element="span" />
    </>
  );
}
