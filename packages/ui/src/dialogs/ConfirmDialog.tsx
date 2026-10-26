import { AlertDialog, Input, Label, TextField } from "@heroui/react";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { InlineReference } from "../components/InlineReference";
import type { AcknowledgementView } from "../errorAlerts";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import { Dialog, useDialogAction, useFocusLeavingWhenDisabled } from "./Dialog";
import { dialogAround } from "./dialogElement";
import { useTrimmedField } from "./useTrimmedField";

export interface ConfirmDialogProps {
  /** A plain name for what the dialog does ("Delete session"), the same whatever it is opened on: the subject goes in
   * `subject`. */
  title: string | React.ReactElement;
  /** The first sentence of the body, naming what the dialog is about ("The session Fix search will be deleted."), with
   * the names in it as `InlineReference` chips. */
  subject?: React.ReactNode;
  /** What the dialog says: a string is one paragraph; a node lays out its own structure. */
  message: React.ReactNode;
  confirmLabel?: string;
  /** What the confirm button reads while `onConfirm` is running, for an action that takes seconds. */
  pendingLabel?: string;
  /** What the button that declines is called. */
  cancelLabel?: string;
  /** Leaves out the Cancel button, for a question whose buttons are all ways of answering it: the
   * dialog's close button and Escape still call `onCancel`, so the way out stays. */
  hideCancel?: boolean;
  destructive?: boolean;
  /** A button before Confirm, after Cancel when there is one, for the way out that is neither: it
   * settles the question itself in `onPress`, so the caller closes the dialog there. */
  otherAction?: { label: string; onPress: () => void };
  /** For a question the user did not ask for, which can come up while they are typing elsewhere:
   * focus starts on the dialog rather than on Confirm, for every subject it asks about, so an Enter
   * meant for something else (or held down past the previous answer) answers nothing. */
  unprompted?: boolean;
  /** A control shown under the message that settles what Confirm does, such as a checkbox widening
   * what the action covers; the caller holds its state and reads it in `onConfirm`. A function is
   * given whether `onConfirm` is running, for a control to freeze while it does. */
  extraControl?: React.ReactNode | ((busy: boolean) => React.ReactNode);
  /** What the action takes along, as body text under the options and over the buttons, so that a line
   * it gains when an option is ticked moves no checkbox. A confirmation with it, like a destructive
   * one, is a warning modal: the warning icon leads its header. */
  consequences?: React.ReactNode;
  /** Holds Confirm, shown as pending, while what it would act on is still being found out. */
  waiting?: boolean;
  /** Disables Confirm while what it would do is refused, with the reason shown in the dialog. */
  confirmDisabled?: boolean;
  /** Names what the dialog currently asks about, for one reused across subjects: a change clears
   * the failure shown for the previous one. */
  resetKey?: string;
  /** What the alert for a failure of `onConfirm` is titled, in place of the generic "Error". */
  errorTitle?: string;
  /** A failure that ends the question's purpose: the dialog turns into a failure modal that says it, and its
   * acknowledgement calls `onCancel` (see `Dialog`'s `failure`). */
  failure?: AcknowledgementView;
  size?: "sm" | "md" | "lg";
  /** For an action that cannot be undone and loses a lot: the word the user has to type before
   * Confirm is enabled, shown in capitals. What is typed shows in capitals too, and is compared
   * ignoring case, so a script without case (Chinese) works unchanged. */
  typeToConfirm?: string;
  /** May reject — the shared error alert reports the failure and this dialog stays open, so the
   * caller does not need its own try/catch around the request. */
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}

export function ConfirmDialog({
  title,
  subject,
  message,
  confirmLabel,
  pendingLabel,
  cancelLabel,
  hideCancel,
  extraControl,
  consequences,
  waiting = false,
  confirmDisabled = false,
  destructive,
  otherAction,
  unprompted,
  resetKey,
  errorTitle,
  failure,
  size,
  typeToConfirm,
  onConfirm,
  onCancel,
}: ConfirmDialogProps): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { busy, run } = useDialogAction(resetKey, errorTitle);
  const [typed, setTyped] = useState("");
  const typedField = useTrimmedField(typed, setTyped);
  const word = typeToConfirm?.toLocaleUpperCase(language);
  const confirmed = !confirmDisabled && (word === undefined || typed.toLocaleUpperCase(language) === word);

  // On mount react-aria would put focus on the dialog anyway, so this matters for each new subject. A
  // layout effect, so focus has moved before the new subject is painted and before a repeated Enter
  // can land on Confirm.
  const confirmRef = useRef<HTMLButtonElement>(null);
  useLayoutEffect(() => {
    if (unprompted) dialogAround(confirmRef)?.focus();
  }, [resetKey]);

  // Confirm held while the dialog is open (a refusal that arrives under it) hands its focus to Cancel,
  // or to the dialog when there is none.
  const cancelRef = useRef<HTMLButtonElement>(null);
  const dialogRef = { get current() { return dialogAround(confirmRef) ?? null; } };
  const fallbackRef = hideCancel ? dialogRef : cancelRef;
  useFocusLeavingWhenDisabled(confirmRef, !confirmed, fallbackRef);
  const opensOnConfirm = !destructive && !unprompted && word === undefined;
  // The `autoFocus` below is react-aria's, which for a press without a pointer (assistive technology,
  // a script) waits for transitions to end and is dropped if focus has moved by then. Opened over
  // another dialog (the question a first dialog's Confirm raised), the dialog's own refocus of a lost
  // focus moves it first, so Confirm takes focus here, after the effects of everything inside.
  useEffect(() => {
    if (opensOnConfirm) (confirmed ? confirmRef : fallbackRef).current?.focus();
  }, []);

  const footer = (
    <>
      {!hideCancel && (
        <Button ref={cancelRef} type="button" variant="secondary" onPress={onCancel} isDisabled={busy} autoFocus={opensOnConfirm && !confirmed}>
          {cancelLabel ?? t("common.cancel")}
        </Button>
      )}
      {otherAction && (
        <Button type="button" variant="secondary" onPress={otherAction.onPress} isDisabled={busy}>
          {otherAction.label}
        </Button>
      )}
      {/* Pending rather than disabled while it runs: a natively disabled button drops the focus the
          press gave it, and the dialog's Escape and Tab containment go with it. Focus starts on it,
          so Enter confirms at once, unless the action is destructive or the question unprompted:
          there Enter must not be the way in, and focus stays on the dialog until the user moves it.
          A typed word takes focus for its field instead. */}
      <Button ref={confirmRef} type="submit" variant={destructive ? "danger" : "primary"} isPending={busy || waiting}
        isDisabled={!confirmed} autoFocus={opensOnConfirm && confirmed}>
        {busy && pendingLabel ? pendingLabel : (confirmLabel ?? t("common.confirm"))}
      </Button>
    </>
  );

  return (
    <Dialog title={title} headerLeading={destructive || consequences !== undefined ? <AlertDialog.Icon status="warning" /> : undefined} onClose={busy ? () => {} : onCancel} footer={footer} failure={failure} resetKey={resetKey} size={size} onSubmit={() => confirmed && !busy && !waiting && void run(onConfirm)} alert>
      {typeof message === "string" ? (
        // The space joins the message's own text node: VoiceOver drops a text node that is only whitespace, and would run
        // the two sentences together.
        <p className="text-sm">{subject}{subject === undefined ? message : ` ${message}`}</p>
      ) : (
        <div className="flex flex-col gap-3 text-sm text-foreground">
          {subject !== undefined && <p>{subject}</p>}
          {message}
        </div>
      )}
      {typeof extraControl === "function" ? extraControl(busy) : extraControl}
      {consequences}
      {word !== undefined && (
        <TextField
          fullWidth
          variant="secondary"
          {...typedField}
          autoFocus
        >
          <Label>
            <Message id="dialog.typeToConfirm.label" params={{ word: <InlineReference dir="ltr" className="italic">{word}</InlineReference> }} />
          </Label>
          {/* Capitals by styling rather than by rewriting the value: rewriting a controlled value
              mid-composition breaks an input method's composition. */}
          <Input dir="ltr" placeholder={t("dialog.typeToConfirm.example", { word })} className="uppercase" autoComplete="off" spellCheck={false} />
        </TextField>
      )}
    </Dialog>
  );
}
