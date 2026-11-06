import { Toast as HeroToast, toast, type ToastContentValue } from "@heroui/react";
import React, { createContext, useContext } from "react";
import type { QueuedToast } from "react-aria-components";

import { definedProps } from "./props";

/** A toast's tone: `accent` for a notice, `danger` for a failure. */
export type ToastVariant = "accent" | "danger";

/** Puts a toast in HeroUI's global toast queue, shown by the `ToastRegion`: `title`, with `description` under it, in
 * `variant`'s tone, dismissing itself after `timeout` milliseconds. `onClose` is told once it is closed, by any means.
 * Returns its key, for `closeToast`. */
export function showToast(title: React.ReactNode, { variant, description, timeout, onClose }: {
  variant: ToastVariant;
  description?: React.ReactNode;
  timeout?: number;
  onClose?: () => void;
}): string {
  return toast(title, { variant, ...definedProps({ description, timeout, onClose }) });
}

/** Closes the toast `showToast` returned `key` for; its `onClose` is told at once. */
export function closeToast(key: string): void {
  toast.close(key);
}

/** What a toast on screen shows, as `showToast` was given it. */
export interface ToastEntry {
  title: React.ReactNode;
  description: React.ReactNode;
  variant: ToastVariant;
}

const QueuedContext = createContext<QueuedToast<ToastContentValue> | null>(null);

/** HeroUI's toast region, the stack the queued toasts show in, drawn by `children` (a `Toast` for each). `ref` is handed
 * the region's element; `placement` is where in the window the stack stands; `hotkey` is the keys that move focus to it
 * (none, given `[]`). */
export function ToastRegion({ ref, placement, hotkey, children }: {
  ref?: React.Ref<HTMLDivElement>;
  placement: "top" | "bottom end";
  hotkey?: string[];
  children: (entry: ToastEntry) => React.ReactElement;
}): React.ReactElement {
  return (
    <HeroToast.Provider ref={ref} placement={placement} {...definedProps({ hotkey })}>
      {({ toast: queued }: { toast: QueuedToast<ToastContentValue> }) => {
        const { title, description, variant } = queued.content;
        return (
          <QueuedContext.Provider value={queued}>
            {children({ title, description, variant: variant === "danger" ? "danger" : "accent" })}
          </QueuedContext.Provider>
        );
      }}
    </HeroToast.Provider>
  );
}

/** HeroUI's toast, the one `ToastRegion` is drawing: its `ToastIndicator`, its `ToastContent` and its
 * `ToastCloseButton`, in `variant`'s tone. */
export function Toast({ variant, children }: { variant: ToastVariant; children: React.ReactNode }): React.ReactElement {
  const queued = useContext(QueuedContext);
  if (!queued) throw new Error("Toast must be drawn by a ToastRegion");
  return <HeroToast toast={queued} variant={variant}>{children}</HeroToast>;
}

/** HeroUI's toast icon, for `variant`. */
export function ToastIndicator({ variant }: { variant: ToastVariant }): React.ReactElement {
  return <HeroToast.Indicator variant={variant} />;
}

/** HeroUI's toast content, holding its `ToastTitle` and `ToastDescription`. */
export function ToastContent({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroToast.Content>{children}</HeroToast.Content>;
}

export function ToastTitle({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroToast.Title>{children}</HeroToast.Title>;
}

/** HeroUI's toast description, a `div` so that it can hold block content. */
export function ToastDescription({ children }: { children: React.ReactNode }): React.ReactElement {
  return <HeroToast.Description elementType="div">{children}</HeroToast.Description>;
}

/** HeroUI's close button of a toast. Its name and description are `label` and `describedBy`, as a `Button`'s;
 * `focusOnPress={false}` leaves focus where it was. */
export function ToastCloseButton({ label, describedBy, focusOnPress }: {
  label: string;
  describedBy?: string;
  focusOnPress?: boolean;
}): React.ReactElement {
  return <HeroToast.CloseButton aria-label={label} {...definedProps({ "aria-describedby": describedBy })} preventFocusOnPress={focusOnPress === false || undefined} />;
}
