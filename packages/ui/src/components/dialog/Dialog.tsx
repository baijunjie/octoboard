import React, { useEffect, useId, useLayoutEffect, useRef, useState } from "react";

import { takeMenuFocusToRestore } from "../focus/menuFocus";
import { FadedScroller, useFadeRegion } from "../scroll/scrollFade";
import { TitledControl } from "../hint/TitledControl";
import { useT } from "../../i18n/react";
import type { AcknowledgementView } from "../alerts/errorAlerts";
import { dialogAround } from "./dialogElement";
import { ErrorAlertBody, alertTitle } from "./ErrorAlertBody";
import { useEscapeWhileTooltipOpen } from "./useEscapeWhileTooltipOpen";
import { useHeightMotion } from "./useHeightMotion";
import { useToastClearance } from "./useToastClearance";
import { useTopLayerAtMount } from "./useTopLayerAtMount";
import { useFocusLeavingWhenDisabled, useRefocusIfLost } from "./dialogHooks";
import { useFocusLeavingWhenBusy } from "./useFocusLeavingWhenBusy";
import { Button, Modal, ModalBody, ModalCloseButton, ModalFooter, ModalHeader, ModalHeading, ModalStatusIcon } from "../../primitives";

const NOT_TYPED_INTO = new Set(["checkbox", "radio", "button", "submit", "image", "reset", "range", "color", "file"]);

/** Whether `target` takes typed text, where a held key's repeats are what the user means. */
function isTextEntry(target: EventTarget): boolean {
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  return target instanceof HTMLInputElement && !NOT_TYPED_INTO.has(target.type);
}

/** A control that holds a choice rather than acting: a closed select, a tab, a radio, a switch. */
const CHOICE_CONTROL = '[aria-haspopup="listbox"]:not([aria-expanded="true"]), [role="tab"], [role="radio"], [role="switch"]';

/**
 * Enter on a control that holds a choice submits the dialog, as it already does from a text field or a
 * checkbox: having made a choice, the user presses Enter to go ahead, not to open the picker again.
 * The control keeps Space and the arrow keys. A button that acts (Cancel, a disclosure) keeps Enter, and
 * so does whatever is open in a popover, which is outside the form's own elements. While the submit
 * button is disabled, or pending (react-aria then makes it a plain button), the key falls back to the
 * control.
 */
function submitsFromChoice(e: React.KeyboardEvent<HTMLFormElement>): boolean {
  if (e.key !== "Enter" || e.repeat) return false;
  const form = e.currentTarget;
  if (!(e.target instanceof Element) || !form.contains(e.target) || !e.target.matches(CHOICE_CONTROL)) return false;
  const submit = form.querySelector<HTMLButtonElement>('button[type="submit"]');
  if (!submit || submit.disabled) return false;
  e.preventDefault();
  e.stopPropagation();
  form.requestSubmit(submit);
  return true;
}

/** How many `Dialog`s are mounted, so a nested one can tell it is not the last to close. */
let openDialogs = 0;

/**
 * The frame every dialog shares. Unless `dismissable` is false, Escape calls `onClose`, which
 * `keyboardDismissHeld` withholds from Escape alone and `busy` from Escape and a click outside. A
 * click outside also closes a non-alert dialog; confirmations require an explicit response. The
 * dialog is mounted only while it is open, so `isOpen` is constant. With `onSubmit` the header, body
 * and footer sit in a `<form>`, so Enter in a text field submits it — the dialog decides what
 * "submit" means. Passing or dropping `onSubmit` while the dialog is open swaps that `<form>` and
 * remounts the header, body and footer, so a dialog that moves through steps keeps `onSubmit` across
 * any step where a pressed button has to keep focus. The footer is Cancel plus a submit button
 * labelled `submitLabel`, unless the dialog brings its own `footer`.
 *
 * While `busy`, the submit button is marked pending rather than disabled: a disabled attribute would blur the button
 * the user has just pressed and take the dialog's Tab with it for as long as the request runs, where pending blocks
 * press and hover, keeps the button focusable and announces itself. Every other control in the dialog is disabled (the
 * wrappers read `Modal`'s `busy`, so Cancel, the close button, the fields and the links of the body all are), and a
 * busy dialog cannot be left at all. Assistive technology still reads the disabled controls: the dialog is never
 * `inert`. Focus that was on a control the request has just disabled moves to the pending button, else to the dialog
 * (`useFocusLeavingWhenBusy`), so it never falls to `<body>`; a pending button of the dialog's own, in its own `footer`
 * or its body, takes it the same.
 *
 * `submitDisabled` disables the submit button alone, before any action, Cancel stays pressable, and
 * since it is the form's only submit control this also suppresses implicit submission from Enter in a
 * text field. Focus is never left on a button that has become disabled: it moves to Cancel. A form
 * dialog puts `autoFocus` on its first text field, so Enter submits at once; one with no text field
 * passes `autoFocusSubmit` to open on the submit button instead, or on Cancel when there is nothing
 * to submit yet (a dialog with its own `footer` puts `autoFocus` on its own button). `Button` and
 * `TextInput` mark that control with `openingFocus`, which is where focus comes back to when an
 * acknowledged alert leaves it nowhere to return (`focusAfterLoss`); a field built from HeroUI's own
 * parts marks itself.
 */
function DialogFrame({
  title,
  headerLeading,
  subtitle,
  onClose,
  children,
  footer,
  submitLabel,
  busy = false,
  submitDisabled,
  autoFocusSubmit,
  onSubmit,
  size = "md",
  alert,
  failing,
  resetKey,
  returnFocusRef,
  dismissable = true,
  keyboardDismissHeld = false,
  focusesItselfOnOpen,
  topLayerAtMount,
  fullScreen = false,
}: {
  /** Text, or an element for a title that needs markup of its own (a file name kept left to right). */
  title: string | React.ReactElement;
  /** Controls beside the title, outside the heading's accessible name. */
  headerLeading?: React.ReactNode;
  /** A line under the title that belongs to it (where a compared change is from): in the header, so
   * it reads after the title and is never clipped by the body's scroller. It brings its own height. */
  subtitle?: React.ReactElement;
  onClose: () => void;
  children: React.ReactNode;
  /** `wide` is `lg` widened for content laid out across it: a list of rows that each carry a path,
   * or groups of fields side by side; `viewer` is a frame for content rather than a form: it fills
   * the window on phones, and otherwise keeps a margin and a maximum readable width. */
  size?: "sm" | "md" | "lg" | "wide" | "viewer";
  /** Uses the modal's native full size, including its edge-to-edge transition. */
  fullScreen?: boolean;
  /** A request the dialog started is out and cannot be taken back, so the dialog cannot be left: every control in it is
   * disabled but a pending button (the close button and Cancel included), and Escape and a click outside do not call
   * `onClose`. With `submitLabel` the submit button is marked pending; a dialog with its own `footer` shows the wait
   * itself, with a pending button of its own where it has an action that is running. */
  busy?: boolean;
  /** A confirmation that interrupts the user: HeroUI's `AlertDialog`, announced as an alert dialog. */
  alert?: boolean;
  /** The dialog is a failure modal, announced and described as an alert dialog whatever frame it is in. */
  failing?: boolean;
  /** An error requires its acknowledgement button, with no implicit dismissal. */
  dismissable?: boolean;
  /** While set, Escape does not close the dialog, whether it reaches react-aria's modal or arrives while a tooltip is
   * open; the Close button and a click outside still do. For a dialog whose content takes Escape for something of its
   * own, closing it from a listener on the dialog element: the dialog must not also close from the same press. */
  keyboardDismissHeld?: boolean;
  /** Where focus goes back to when the busy period ends, in place of the control it was moved from (see
   * `useFocusLeavingWhenBusy`): the dialog sets it while the request runs, for a refusal that is about one field, and
   * the frame clears it. */
  returnFocusRef?: React.MutableRefObject<HTMLElement | null>;
  /** Names what a dialog reused across subjects currently asks about; when it changes and focus
   * was lost with the old content, it is put back on the dialog. */
  resetKey?: string;
  /** The caller puts focus on the dialog itself, in the render after the one that mounted it, once react-aria has
   * turned the modal's focus containment on (from a layout effect at mount, which forces a render), so the frame never
   * puts focus found on `<body>` back on the dialog when `resetKey` changes: a caller that passes this
   * with a `resetKey` puts focus back itself for each new subject. Until then the frame holds focus on an empty
   * element inside the dialog from its mount: react-aria takes focus off a dialog that finds focus outside it when the
   * mount's passive effects run, half a second later, and puts it back only once no transition is running on the page,
   * so one that never reports its end (in Chrome, one still running inside a disclosure as it folds away) would leave
   * focus on the page body. The empty element rather than the dialog itself, for a modal that mounts in the same commit
   * (one that a dialog opens from an effect as it mounts): with the dialog itself focused at mount, focus ends on this
   * dialog's first control or on that modal's opening control, while focus moved on from the empty element stays put. A
   * dialog already open underneath plays no part in that while this one is also `topLayerAtMount`, as react-aria never
   * counts focus inside a top layer as escaping its containment. */
  focusesItselfOnOpen?: boolean;
  /** Marks the overlay, the element portalled to `<body>`, a react-aria top layer (`data-react-aria-top-layer`) for
   * the commit it mounts in, and only that commit (see `useTopLayerAtMount`). For a modal that comes up a commit after
   * what raised it, which can be the very commit another modal mounts in: one that a dialog opens from an effect as it
   * mounts. */
  topLayerAtMount?: boolean;
} & (
  | {
    /** `null` for a dialog without a footer. */
    footer: React.ReactNode | null;
    submitLabel?: never;
    submitDisabled?: never;
    autoFocusSubmit?: never;
    onSubmit?: () => void;
  }
  | { footer?: never; submitLabel: string; submitDisabled?: boolean; autoFocusSubmit?: boolean; onSubmit: () => void }
)): React.ReactElement {
  const t = useT();
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
      // Decided a turn later: a dialog that replaces this one in the same commit (the login dialog's running form
      // giving way to its failure form) has mounted by then, so the menu's focus is not taken from it.
      setTimeout(() => {
        if (openDialogs > 0) return;
        const element = takeMenuFocusToRestore();
        if (element?.isConnected) element.focus();
      }, 0);
    };
  }, []);

  const markerRef = useRef<HTMLSpanElement>(null);
  const submitRef = useRef<HTMLButtonElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const ownReturnFocus = useRef<HTMLElement | null>(null);
  const dialog = () => dialogAround(markerRef);
  useFocusLeavingWhenDisabled(submitRef, submitDisabled === true, cancelRef, dialog);
  useFocusLeavingWhenBusy(dialog, busy, returnFocusRef ?? ownReturnFocus, resetKey);
  useRefocusIfLost(() => (focusesItselfOnOpen ? null : dialog()), [resetKey]);
  const landingRef = useRef<HTMLSpanElement>(null);
  useLayoutEffect(() => {
    landingRef.current?.focus({ preventScroll: true });
  }, []);
  useToastClearance(markerRef, footer !== null);
  useHeightMotion(markerRef);
  useEscapeWhileTooltipOpen(dialog, dismissable && !keyboardDismissHeld && !busy ? onClose : () => {});
  const topLayer = useTopLayerAtMount(topLayerAtMount === true, markerRef);

  const alerting = alert || failing;
  // An alert dialog is described by its whole body, so a screen reader announces what is asked
  // together with the title; react-aria only wires up a `slot="description"` Text, which this body
  // is not.
  const bodyId = useId();
  // The body's scroll container is reachable from the keyboard while it scrolls: a dialog whose
  // controls are all in the footer (a long message, a page of text) has nothing in the body to
  // take focus, and the arrow keys and Page Down would otherwise move nothing. It is named by the
  // dialog's own title, which react-aria has already tied to the dialog.
  const [titleId, setTitleId] = useState<string>();
  useEffect(() => setTitleId(dialogAround(markerRef)?.getAttribute("aria-labelledby") ?? undefined), []);
  const region = useFadeRegion({ labelledBy: titleId });

  const body = (
    <>
      {/* The body is not the scroller itself: the `FadedScroller` inside it is, so that every
          dialog whose content outgrows the window fades at the edge it can still be scrolled
          towards, with no caller building a scroll container of its own. The fade is a mask that
          shows only while there is more to scroll and takes no pointer events, and it is a hint
          alone: the scroller is an ordinary overflow container, so the keyboard, the wheel and
          focus moving into view scroll it as they would any other. The padding is the room a focus
          ring needs inside the clip. */}
      {/* HeroUI's body is muted grey; prose in a dialog reads in the foreground, and a note that is
          secondary says so itself (`Description`, `text-app-muted`). */}
      {/* The ring of the focusable scroller is drawn on its parent: the fade is a mask on the scroller
            and clips by its border box, so a ring on the scroller itself, which sits outside that box, is
            cut off. The ring is the project's usual one (2px `--focus`), from the scroller's own state. */}
      {/* Size the body from its content: a percentage flex basis can leave WebKit holding the body at
          an intermediate size after the dialog's height animation, even while a disclosure grows.
          The viewer keeps its fixed-height layout. */}
      <ModalBody
        id={bodyId}
        className={`text-foreground ${size === "viewer" ? `relative m-0 flex flex-col overflow-hidden p-0 ${region.overlayRing}` : `flex basis-auto flex-col overflow-y-visible rounded-lg ${region.ring}`}`}
      >
        <FadedScroller region={region} className={size === "viewer" ? "flex min-h-0 flex-1 flex-col" : "flex min-h-0 flex-1 flex-col gap-4 p-1"}>
          {children}
          {/* Out of the flow, so the body's gap leaves it out, and hidden from assistive technology, so the virtual
              cursor does not stop on it: it holds focus only within the task that mounts the dialog. */}
          {focusesItselfOnOpen && <span ref={landingRef} tabIndex={-1} aria-hidden="true" className="absolute" />}
        </FadedScroller>
      </ModalBody>
      {footer !== null && (
        <ModalFooter className="flex-wrap">
          {footer ?? (
            <>
              <Button ref={cancelRef} type="button" variant="secondary" onPress={onClose} autoFocus={autoFocusSubmit && submitDisabled}>
                {t("common.cancel")}
              </Button>
              <Button ref={submitRef} type="submit" pending={busy} disabled={submitDisabled} autoFocus={autoFocusSubmit && !submitDisabled}>
                {submitLabel}
              </Button>
            </>
          )}
        </ModalFooter>
      )}
    </>
  );

  const heading = <ModalHeading className="min-w-0 [overflow-wrap:anywhere]">{title}</ModalHeading>;
  // The fullscreen viewer pulls its close button closer to the corner, so its heading keeps the
  // same gap to it with a narrower inset and a wider end padding. Only one end padding is emitted:
  // HeroUI merges the header's classes and would keep just the last.
  // HeroUI's alert dialog has no `full` size: only the viewer, never an alert, goes full screen, and its frame is then
  // the modal.
  const containerSize = fullScreen ? "full" : size === "viewer" || size === "wide" ? "lg" : size;
  const headerInset = size !== "viewer" ? "" : fullScreen ? "mx-3 mt-3 " : "mx-6 mt-6 ";
  const headerEndPadding = size === "viewer" && fullScreen ? "pe-8" : "pe-6";
  // Keep the modal parts together: HeroUI spaces them through adjacent-sibling selectors.
  const content = (
    <>
      {/* HeroUI places the close button over the header's end without reserving room for it, so
          the heading is padded clear of it; a long title (a session's name, say) has to wrap
          anywhere, since a name with no place to break would run out of the dialog. */}
      <ModalHeader className={`${headerInset}${headerLeading ? "grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2 gap-y-1 " : subtitle ? "gap-1 " : ""}${headerEndPadding}`}>
        {headerLeading}
        {heading}
        {subtitle}
      </ModalHeader>
      {body}
    </>
  );

  return (
    // The viewer fills narrow screens. Between tablet and desktop it keeps a 16px margin; from desktop up HeroUI supplies
    // 40px. Its height also leaves room for the bottom banners, matching the container's inset in `style.css`.
    <Modal
      alert={alert}
      busy={busy}
      size={containerSize}
      outsideDismissable={dismissable && !alert}
      keyboardDismissDisabled={!dismissable || keyboardDismissHeld || busy}
      onClose={() => dismissable && !busy && onClose()}
      topLayer={topLayer}
      containerClassName={!fullScreen && size === "viewer" ? "max-desktop:p-4" : undefined}
      role={failing ? "alertdialog" : undefined}
      describedBy={alerting ? bodyId : undefined}
      className={
        fullScreen
          ? "control-fills bg-panel-sidebar overflow-hidden p-0"
          : size === "viewer"
            ? "control-fills bg-panel-sidebar overflow-hidden p-0 h-[calc(100dvh-32px-var(--bottom-chrome-height))] w-[calc(100vw-32px)] max-w-none desktop:h-[calc(100dvh-80px-var(--bottom-chrome-height))] desktop:w-[min(1440px,calc(100vw-80px))]"
            : size === "wide"
              ? "control-fills max-w-3xl"
              : "control-fills"
      }
    >
      <span ref={markerRef} hidden />
      {dismissable && <TitledControl title={t("common.close")}>
        {/* Named explicitly so it always matches the tooltip. */}
        <ModalCloseButton label={t("common.close")} className={fullScreen ? "top-2 end-3" : undefined} />
      </TitledControl>}
      {onSubmit ? (
      // A key held down repeats: the press that opened the dialog (a menu item takes Enter and
      // Space on keydown) would otherwise go on to submit it before the user has read it, Enter
      // on a repeat, Space on its release. Caught on the way down, since react-aria's buttons
      // stop the keydown from bubbling. Where text is typed the repeats are the user's own, so
      // they stay: Space in any text field, Enter in a textarea.
        <form
          className="contents"
          onKeyDownCapture={(e) => {
            if (submitsFromChoice(e)) return;
            if (!e.repeat) return;
            const enter = e.key === "Enter" && !(e.target instanceof HTMLTextAreaElement);
            if (enter || (e.key === " " && !isTextEntry(e.target))) e.preventDefault();
          }}
          onSubmit={(e) => {
            e.preventDefault();
            // React bubbles a submit through portals, so a form in a dialog opened from inside
            // this one would otherwise submit this one too.
            e.stopPropagation();
            onSubmit();
          }}
        >
          {content}
        </form>
      ) : (
        content
      )}
    </Modal>
  );
}

type DialogFrameProps = Parameters<typeof DialogFrame>[0];

/**
 * The frame every dialog shares (see `DialogFrame`'s props). With `failure` the dialog turns into a failure modal in
 * place: a failure that ends the dialog's purpose replaces its content, in the same modal, with the error alert
 * dialog's form — the danger icon, the failure's title, its explanation and "Error details", and the acknowledgement
 * as the only button, which calls `onClose`. It is an acknowledgement-only failure: there is no Report issue button,
 * and the type of `failure` leaves out an unexpected failure, whose report-a-bug guidance would have nothing to act on
 * here. Only that acknowledgement closes it. The dialog's own title, body, footer and submit are not shown.
 */
export function Dialog({ failure, ...props }: DialogFrameProps & {
  failure?: AcknowledgementView;
}): React.ReactElement {
  const t = useT();
  if (!failure) return <DialogFrame {...(props as DialogFrameProps)} />;
  return (
    <DialogFrame
      failing
      size={props.size}
      fullScreen={props.fullScreen}
      alert={props.alert}
      topLayerAtMount={props.topLayerAtMount}
      resetKey={failure.message}
      dismissable={false}
      onClose={props.onClose}
      headerLeading={<ModalStatusIcon status="danger" />}
      title={alertTitle(t, failure)}
      footer={<Button autoFocus variant="danger" onPress={props.onClose}>{t("error.acknowledge")}</Button>}
    >
      <ErrorAlertBody alert={failure} />
    </DialogFrame>
  );
}
