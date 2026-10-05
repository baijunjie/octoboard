import { Button, Modal } from "@heroui/react";
import React, { useEffect, useRef, useState } from "react";

import { takeMenuFocusToRestore } from "../components/ActionMenu";

/** The inline error and busy state every dialog that talks to the daemon needs. `run` clears the
 * error, marks the dialog busy for the duration of `action`, and shows a rejection as the error
 * instead of letting it escape, so the dialog stays open for another try. A dialog reused for
 * successive subjects passes a `resetKey` naming the current one: when it changes the error and
 * busy state are cleared, and an action still running for the previous subject no longer reports
 * into them. */
export function useDialogAction(resetKey?: string): {
  error: string | undefined;
  setError: (message: string | undefined) => void;
  busy: boolean;
  run: (action: () => Promise<void>) => Promise<void>;
} {
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [seenKey, setSeenKey] = useState(resetKey);
  const currentKey = useRef(resetKey);
  currentKey.current = resetKey;
  if (seenKey !== resetKey) {
    setSeenKey(resetKey);
    setError(undefined);
    setBusy(false);
  }

  const run = async (action: () => Promise<void>) => {
    const startedFor = resetKey;
    const stale = () => currentKey.current !== startedFor;
    setBusy(true);
    setError(undefined);
    try {
      await action();
    } catch (err) {
      if (!stale()) setError((err as Error).message);
    } finally {
      if (!stale()) setBusy(false);
    }
  };

  return { error, setError, busy, run };
}

/** Puts keyboard focus back on the element `target` returns when `deps` change and focus has fallen to `<body>`,
 * which is where it goes when the focused control is unmounted by the change. Left alone, a dialog's
 * Escape handling and Tab containment stop working, since they hang off focus being inside it. */
export function useRefocusIfLost(target: () => HTMLElement | null | undefined, deps: React.DependencyList): void {
  useEffect(() => {
    if (document.activeElement === document.body) target()?.focus();
  }, deps);
}

/** A dialog's inline failure line. */
export function DialogError({ message }: { message: string | undefined }): React.ReactElement | null {
  return message ? (
    <p role="alert" className="text-sm text-danger">
      {message}
    </p>
  ) : null;
}

/** The children HeroUI's modal parts accept. Their typings resolve `ReactNode` against the React 18
 * types that react-aria pulls in, which `React.ReactNode` from this package's React 19 does not
 * satisfy. */
type BodyChildren = React.ComponentProps<typeof Modal.Footer>["children"];

/** How many `Dialog`s are mounted, so a nested one can tell it is not the last to close. */
let openDialogs = 0;

/**
 * The frame every dialog shares. Escape and a click outside both call `onClose`; the dialog is
 * mounted only while it is open, so `isOpen` is constant. With `onSubmit` the body and footer sit in
 * a `<form>`, so Enter in a text field submits it — the dialog decides what "submit" means. The
 * footer is Cancel plus a submit button labelled `submitLabel`, both disabled while `busy`, unless
 * the dialog brings its own `footer`.
 */
export function Dialog({
  title,
  onClose,
  children,
  footer,
  submitLabel,
  busy,
  onSubmit,
  size = "md",
  alert,
  resetKey,
}: {
  title: string;
  onClose: () => void;
  children: BodyChildren;
  size?: "sm" | "md" | "lg";
  /** A confirmation that interrupts the user, announced as an alert dialog. */
  alert?: boolean;
  /** Names what a dialog reused across subjects currently asks about; when it changes and focus
   * was lost with the old content, it is put back on the dialog. */
  resetKey?: string;
} & (
  | { footer: BodyChildren; submitLabel?: never; busy?: never; onSubmit?: () => void }
  | { footer?: never; submitLabel: string; busy?: boolean; onSubmit: () => void }
)): React.ReactElement {
  // Closing returns focus to the element focused when the dialog opened, which for a dialog a menu
  // item opened is the menu's trigger. The menu knows better (the terminal, for a pointer press),
  // so that wins. Whichever runs first, the menu's element ends up focused: react-aria's own
  // restore is delayed a frame and acts only while focus is on `<body>`, and one that runs earlier
  // is overridden by this move. A dialog opened from inside another one (the directory picker)
  // leaves it for the outer dialog, which is the one the menu opened.
  useEffect(() => {
    openDialogs += 1;
    return () => {
      openDialogs -= 1;
      if (openDialogs > 0) return;
      const element = takeMenuFocusToRestore();
      if (element) setTimeout(() => element.isConnected && element.focus(), 0);
    };
  }, []);

  // HeroUI's `Modal.Dialog` takes no ref, so the dialog element is found from a marker inside it.
  const markerRef = useRef<HTMLSpanElement>(null);
  useRefocusIfLost(() => markerRef.current?.closest<HTMLElement>("[role=dialog], [role=alertdialog]"), [resetKey]);

  const body = (
    <>
      <Modal.Body className="flex flex-col gap-4 p-1">{children}</Modal.Body>
      <Modal.Footer className="flex-wrap">
        {footer ?? (
          <>
            <Button type="button" variant="secondary" onPress={onClose} isDisabled={busy}>
              Cancel
            </Button>
            <Button type="submit" isDisabled={busy}>
              {submitLabel}
            </Button>
          </>
        )}
      </Modal.Footer>
    </>
  );

  return (
    <Modal.Backdrop isOpen onOpenChange={(open) => !open && onClose()}>
      <Modal.Container size={size}>
        <Modal.Dialog role={alert ? "alertdialog" : "dialog"}>
          <span ref={markerRef} hidden />
          <Modal.CloseTrigger />
          <Modal.Header>
            <Modal.Heading>{title}</Modal.Heading>
          </Modal.Header>
          {onSubmit ? (
            <form
              className="contents"
              onSubmit={(e) => {
                e.preventDefault();
                onSubmit();
              }}
            >
              {body}
            </form>
          ) : (
            body
          )}
        </Modal.Dialog>
      </Modal.Container>
    </Modal.Backdrop>
  );
}
