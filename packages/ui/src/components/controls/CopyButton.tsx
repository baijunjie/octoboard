import { Copy } from "lucide-react";
import React, { useRef } from "react";

import { Button } from "../../primitives";
import { useT } from "../../i18n/react";
import { useCopyWithNotice } from "../host/useCopyWithNotice";
import { FeedbackButton, useDoneFeedback } from "./FeedbackButton";

/**
 * An icon button that puts `text` on the clipboard and says so where the user is looking: its
 * tooltip ("Copy", or `beforeCopyText`) turns into "Copied" (or `afterCopyText`) and stays open
 * through the press, and its icon changes from the copy glyph to a check that fades in. Once the
 * tooltip has closed it goes back to the first state, the copy glyph fading in again. "Copied" is
 * also announced through a status region, since the accessible name of a control that keeps focus
 * is not reliably re-read.
 *
 * It is icon-only unless given `children`, which are drawn as a label before the icon; the other
 * props are the `Button`'s (`size`, `variant`, `className`, `focusOnPress`) and the caller
 * sets them to fit the place it sits in. An icon-only button's accessible name is the tooltip's
 * text. Where the platform has no clipboard, or it refuses, it copies through a selection (see
 * `copyText`). A copy that fails opens the error alert titled `failedText`.
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
  /** The error alert's title when the copy fails. */
  failedText: string;
  /** A label drawn before the icon. */
  children?: React.ReactNode;
} & Omit<React.ComponentProps<typeof Button>, "children" | "onPress" | "iconOnly">): React.ReactElement {
  const t = useT();
  const copyWithNotice = useCopyWithNotice();
  const feedback = useDoneFeedback();
  const buttonRef = useRef<HTMLButtonElement>(null);

  const copy = () => {
    copyWithNotice(text, { failed: failedText, anchor: buttonRef.current, onCopied: feedback.markDone });
  };

  return (
    <FeedbackButton
      {...buttonProps}
      feedback={feedback}
      idleText={beforeCopyText ?? t("common.copy")}
      doneText={afterCopyText ?? t("common.copied")}
      icon={Copy}
      buttonRef={buttonRef}
      onPress={copy}
    >
      {children}
    </FeedbackButton>
  );
}
