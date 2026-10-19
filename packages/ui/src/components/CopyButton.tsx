import { Button } from "@heroui/react";
import { Check, Copy } from "lucide-react";
import React, { useEffect, useRef, useState } from "react";

import { ActionableError } from "../errorRecovery";
import { useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import { useDaemon } from "../store";
import { copyText } from "./copyText";
import { StatusAnnouncer } from "./StatusAnnouncer";
import { TitledControl } from "./TitledControl";

/** How long the copied state outlasts the tooltip's close, so the tooltip's exit animation (100 ms)
 * is over before the text and icon go back, and the user never sees them flip while it fades. */
const RESET_AFTER_TOOLTIP_CLOSE_MS = 150;

/** How long the copied state lasts when no tooltip is open to close it (a press that did not show
 * one, such as a tap). */
const COPIED_HOLD_MS = 2000;

/**
 * An icon button that puts `text` on the clipboard and says so where the user is looking: its
 * tooltip ("Copy", or `beforeCopyText`) turns into "Copied" (or `afterCopyText`) and stays open
 * through the press, and its icon changes from the copy glyph to a check that fades in. Once the
 * tooltip has closed it goes back to the first state, the copy glyph fading in again. "Copied" is
 * also announced through a status region, since the accessible name of a control that keeps focus
 * is not reliably re-read.
 *
 * It is icon-only unless given `children`, which are drawn as a label before the icon; the other
 * props are the `Button`'s (`size`, `variant`, `className`, `preventFocusOnPress`) and the caller
 * sets them to fit the place it sits in. An icon-only button's accessible name is the tooltip's
 * text. Where the platform has no clipboard, or it refuses, it copies through a selection (see
 * `copyText`). A copy that fails raises an error toast saying `failedText`.
 */
export function CopyButton({
  text,
  beforeCopyText,
  afterCopyText,
  failedText,
  children,
  ...buttonProps
}: {
  /** What goes on the clipboard. */
  text: string;
  /** The tooltip before copying; "Copy" by default. */
  beforeCopyText?: string;
  /** The tooltip once copied; "Copied" by default. */
  afterCopyText?: string;
  /** What the error toast says when the copy fails; "Could not copy" by default. */
  failedText?: string;
  /** A label drawn before the icon. */
  children?: React.ReactNode;
} & Omit<React.ComponentProps<typeof Button>, "children" | "onPress" | "isIconOnly">): React.ReactElement {
  const t = useT();
  const { clipboard } = usePlatform();
  const { toastError } = useDaemon();
  const [copied, setCopied] = useState(false);
  // Whether the check has been shown, so the copy glyph fades in on the way back but not on mount.
  const [everCopied, setEverCopied] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const resetTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const tooltipOpen = useRef(false);
  useEffect(() => () => clearTimeout(resetTimer.current), []);

  const copiedText = afterCopyText ?? t("common.copied");
  const tooltipText = copied ? copiedText : (beforeCopyText ?? t("common.copy"));
  const iconOnly = children === undefined || children === null || children === false;

  const copy = () => {
    copyText(text, { clipboard, anchor: buttonRef.current }).then(
      () => {
        clearTimeout(resetTimer.current);
        setCopied(true);
        setEverCopied(true);
        if (!tooltipOpen.current) resetTimer.current = setTimeout(() => setCopied(false), COPIED_HOLD_MS);
      },
      (cause: unknown) => toastError(new ActionableError(failedText ?? t("common.copyTextFailed"), "clipboard", { cause })),
    );
  };

  const onTooltipOpenChange = (isOpen: boolean) => {
    tooltipOpen.current = isOpen;
    clearTimeout(resetTimer.current);
    if (!isOpen) resetTimer.current = setTimeout(() => setCopied(false), RESET_AFTER_TOOLTIP_CLOSE_MS);
  };

  return (
    <>
      <TitledControl title={tooltipText} keepOpenOnPress onOpenChange={onTooltipOpenChange}>
        <Button {...buttonProps} ref={buttonRef} isIconOnly={iconOnly} aria-label={iconOnly ? tooltipText : buttonProps["aria-label"]} onPress={copy}>
          {children}
          {copied
            ? <Check key="copied" aria-hidden="true" className="size-4 motion-safe:animate-copy-icon-in" />
            : <Copy key="copy" aria-hidden="true" className={everCopied ? "size-4 motion-safe:animate-copy-icon-in" : "size-4"} />}
        </Button>
      </TitledControl>
      <StatusAnnouncer text={copied ? copiedText : undefined} element="span" />
    </>
  );
}
