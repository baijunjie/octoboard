import { ClipboardPaste } from "lucide-react";
import React from "react";

import { ActionableError } from "../../errors/actionableError";
import { useT } from "../../i18n/react";
import { useOptionalPlatform } from "../../platform/react";
import { Button } from "../../primitives";
import { raiseError } from "../alerts/raiseError";
import { FeedbackButton, useDoneFeedback } from "./FeedbackButton";

/**
 * An icon button that reads the clipboard's text, only because the user pressed it, and hands it to `onPaste`; the
 * counterpart of `CopyButton`, which it mirrors: its tooltip ("Paste", or `beforePasteText`) turns into "Pasted" (or
 * `afterPasteText`) and stays open through the press, its icon changes from the paste glyph to a check, and "Pasted" is
 * announced through a status region. It is icon-only unless given `children`, and takes the `Button`'s other props
 * (`size`, `variant`, `className`, `focusOnPress`) for the place it sits in.
 *
 * The text is read through the platform's clipboard (`ClipboardCapability.readText`), never the page's own, which a
 * desktop webview answers with a system Paste bubble. Where the platform cannot read the clipboard the button is not
 * drawn. A clipboard with no text (empty, or only an image) is nothing to paste: nothing happens and nothing is said.
 * A read the environment refuses, or that fails, is an error the user's own action caused, and opens the error alert
 * titled `failedText`.
 */
export function PasteButton({
  onPaste,
  beforePasteText,
  afterPasteText,
  failedText,
  children,
  ...buttonProps
}: {
  /** Called with the clipboard's text, which is not empty. */
  onPaste: (text: string) => void;
  /** The tooltip before pasting; "Paste" by default. */
  beforePasteText?: string;
  /** The tooltip once pasted; "Pasted" by default. */
  afterPasteText?: string;
  /** The error alert's title when the clipboard cannot be read. */
  failedText: string;
  /** A label drawn before the icon. */
  children?: React.ReactNode;
} & Omit<React.ComponentProps<typeof Button>, "children" | "onPress" | "iconOnly">): React.ReactElement | null {
  const t = useT();
  const clipboard = useOptionalPlatform()?.clipboard;
  const feedback = useDoneFeedback();
  if (!clipboard?.readText) return null;
  const readText = clipboard.readText;

  const paste = () => {
    readText().then(
      (text) => {
        if (text === "") return;
        onPaste(text);
        feedback.markDone();
      },
      (cause: unknown) => raiseError(new ActionableError(t("common.pasteFailedCause"), "none", { cause }), { title: failedText }),
    );
  };

  return (
    <FeedbackButton
      {...buttonProps}
      feedback={feedback}
      idleText={beforePasteText ?? t("common.paste")}
      doneText={afterPasteText ?? t("common.pasted")}
      icon={ClipboardPaste}
      onPress={paste}
    >
      {children}
    </FeedbackButton>
  );
}
