import { AlertDialog, Modal as HeroModal } from "@heroui/react";
import React, { createContext, useContext } from "react";

import { BusyContext, useDisabled } from "./busy";
import { type DataAttributes, definedProps } from "./props";

/** The children HeroUI's modal parts accept. Their typings resolve `ReactNode` against the React 18 types that react-aria
 * pulls in, which `React.ReactNode` from this package's React 19 does not satisfy. */
type PartChildren = React.ComponentProps<typeof HeroModal.Footer>["children"];

/** A modal's size: HeroUI's container sizes; `full` is its native full size, with an edge-to-edge transition, which an
 * alert dialog does not have. */
export type ModalSize = "sm" | "md" | "lg" | "full";

/** Whether the modal the parts sit in is an alert dialog, whose parts are HeroUI's alert dialog's. */
const AlertContext = createContext(false);

function useFrame(): typeof HeroModal | typeof AlertDialog {
  return useContext(AlertContext) ? AlertDialog : HeroModal;
}

/** HeroUI's modal, always open while mounted: its backdrop, its container and its dialog, holding `children` (a
 * `ModalHeader`, a `ModalBody`, a `ModalFooter`, a `ModalCloseButton`, or content of the caller's own). With `alert` it
 * is HeroUI's alert dialog, a confirmation that interrupts the user, announced as an alert dialog.
 *
 * `onClose` is told when the user dismisses it: a press outside, if `outsideDismissable` (by default for a modal, not
 * for an `alert`), or Escape, unless `keyboardDismissDisabled` (by default for an `alert`, not for a modal).
 *
 * While `busy`, every control in it is disabled but a pending `Button` (see `busy.ts`), whatever it is built from: the
 * wrappers read it as their `disabled`, and `useBusy` is there for a control that is not one. Assistive technology
 * still reads them, so the modal is never made `inert`. A modal opened from inside it is its own, not busy. */
export function Modal({
  alert = false,
  busy = false,
  size,
  outsideDismissable = !alert,
  keyboardDismissDisabled = alert,
  onClose,
  topLayer,
  containerClassName,
  role,
  label,
  describedBy,
  className,
  children,
}: {
  alert?: boolean;
  busy?: boolean;
  size?: ModalSize;
  outsideDismissable?: boolean;
  keyboardDismissDisabled?: boolean;
  onClose: () => void;
  /** Marks the overlay, the element portalled to `<body>`, a react-aria top layer (`data-react-aria-top-layer`). */
  topLayer?: boolean;
  containerClassName?: string;
  /** `alertdialog` announces a modal that is not `alert` as an alert dialog. */
  role?: "alertdialog";
  /** The accessible name, for a dialog with no `ModalHeading`. */
  label?: string;
  describedBy?: string;
  className?: string;
  children: React.ReactNode;
}): React.ReactElement {
  const Frame = alert ? AlertDialog : HeroModal;
  // HeroUI's alert dialog has no `full` size, so the union of the two containers' props rejects it; the caller keeps
  // `full` to the modal.
  const containerSize = size as React.ComponentProps<typeof AlertDialog.Container>["size"];
  return (
    <AlertContext.Provider value={alert}>
      <BusyContext.Provider value={busy}>
        <Frame.Backdrop
          isOpen
          isDismissable={outsideDismissable}
          isKeyboardDismissDisabled={keyboardDismissDisabled}
          onOpenChange={(open) => !open && onClose()}
          data-react-aria-top-layer={topLayer ? "true" : undefined}
        >
          <Frame.Container {...definedProps({ size: containerSize, className: containerClassName })}>
            <Frame.Dialog {...definedProps({ role, "aria-label": label, "aria-describedby": describedBy, className })}>
              {children as PartChildren}
            </Frame.Dialog>
          </Frame.Container>
        </Frame.Backdrop>
      </BusyContext.Provider>
    </AlertContext.Provider>
  );
}

/** HeroUI's modal header, holding the `ModalHeading` and what stands beside it. */
export function ModalHeader({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  const Frame = useFrame();
  return <Frame.Header {...definedProps({ className })}>{children as PartChildren}</Frame.Header>;
}

/** HeroUI's modal heading, which names the dialog. */
export function ModalHeading({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  const Frame = useFrame();
  return <Frame.Heading {...definedProps({ className })}>{children as PartChildren}</Frame.Heading>;
}

/** HeroUI's modal body. */
export function ModalBody({ id, className, children }: { id?: string; className?: string; children: React.ReactNode }): React.ReactElement {
  const Frame = useFrame();
  return <Frame.Body {...definedProps({ id, className })}>{children as PartChildren}</Frame.Body>;
}

/** HeroUI's modal footer, holding the dialog's actions. */
export function ModalFooter({ className, children }: { className?: string; children: React.ReactNode }): React.ReactElement {
  const Frame = useFrame();
  return <Frame.Footer {...definedProps({ className })}>{children as PartChildren}</Frame.Footer>;
}

/** HeroUI's close button of a modal, over the end of its header, which closes it as Escape would. Its name and
 * description are `label` and `describedBy`, as a `Button`'s. */
export function ModalCloseButton({ label, describedBy, disabled, className, ...data }: DataAttributes & {
  label: string;
  describedBy?: string;
  disabled?: boolean;
  className?: string;
}): React.ReactElement {
  const Frame = useFrame();
  return <Frame.CloseTrigger {...data} {...definedProps({ "aria-label": label, "aria-describedby": describedBy, isDisabled: useDisabled(disabled), className })} />;
}

/** HeroUI's alert dialog icon, in the colour of its `status`: the mark at the start of a dialog's header that a failure
 * (`danger`) or a consequential confirmation (`warning`) carries. It stands in either kind of modal. */
export function ModalStatusIcon({ status }: { status: "danger" | "warning" }): React.ReactElement {
  return <AlertDialog.Icon status={status} />;
}
