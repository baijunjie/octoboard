import { buttonVariants } from "@heroui/styles";
import React, { useRef, useState } from "react";

import { ActionableError } from "../../errors/actionableError";
import type { Translate } from "../../i18n/catalog";
import { useT } from "../../i18n/react";
import { useOptionalPlatform } from "../../platform/react";
import { ExternalLinkIcon } from "./ExternalLinkIcon";
import { useFocusVisibleProps } from "../focus/useFocusVisibleProps";
import { settle } from "../hooks/settle";
import { raiseError } from "../alerts/raiseError";

export { ExternalLinkIcon } from "./ExternalLinkIcon";

type Props = Omit<React.AnchorHTMLAttributes<HTMLAnchorElement>, "href" | "onClick" | "onAuxClick" | "target" | "rel"> & {
  href: string;
  appearance?: "text" | "button";
  /** The button appearance's size, matching the adjacent controls. */
  size?: "xs" | "sm";
  variant?: "secondary" | "outline";
  onOpen?: (href: string) => void | Promise<void>;
  /** Takes the failure to open the link in place of the default alert, which names the address for the user to open by hand. */
  onFailure?: (cause: unknown) => void;
  isPending?: boolean;
};

/** The body of an alert about an address that could not be opened: `message` and the address as plain text to copy. */
export function addressUnavailableContent(message: string, address: string): React.ReactElement {
  return <div className="flex flex-col gap-4"><p>{message}</p><p dir="ltr" className="font-mono [overflow-wrap:anywhere]">{address}</p></div>;
}

/** Native menu and terminal links share the anchor's visible failure and manual destination. */
export function reportExternalLinkFailure(t: Translate, href: string, cause: unknown): void {
  raiseError(new ActionableError(t("externalLink.unavailable"), "openExternal", { cause }), { title: t("externalLink.unavailableTitle"), content: addressUnavailableContent(t("externalLink.unavailable"), href) });
}

/** Opens an external address without prescribing how its link is drawn. */
export function useExternalLinkAction({ href, onOpen, onFailure, isPending = false }: Pick<Props, "href" | "onOpen" | "onFailure" | "isPending">) {
  const t = useT();
  const platform = useOptionalPlatform();
  const opening = useRef(false);
  const [pending, setPending] = useState(false);
  let address: string | undefined;
  try {
    const url = new URL(href);
    if (["http:", "https:", "mailto:"].includes(url.protocol)) address = url.href;
  } catch {
    // Invalid destinations remain readable rather than becoming an executable or relative URL.
  }
  const busy = isPending || pending;
  const intercepted = !!onOpen || !!platform?.links || platform?.kind === "tauri";
  const activate = async () => {
    if (!address || opening.current || busy) return;
    opening.current = true;
    setPending(true);
    const links = platform?.links;
    const open = onOpen ? () => onOpen(address) : links ? () => links.open(address) : () => Promise.reject(new Error(t("externalLink.unavailable")));
    await settle(open, onFailure ?? ((cause) => reportExternalLinkFailure(t, address, cause)), () => {
      opening.current = false;
      setPending(false);
    });
  };
  return {
    address,
    busy,
    onClick: (event: React.MouseEvent<HTMLAnchorElement>) => {
      if (busy || opening.current) { event.preventDefault(); return; }
      if (!intercepted) return;
      event.preventDefault();
      void activate();
    },
    onAuxClick: (event: React.MouseEvent<HTMLAnchorElement>) => {
      if (busy || opening.current) { event.preventDefault(); return; }
      if (!intercepted || event.button !== 1) return;
      event.preventDefault();
      void activate();
    },
  };
}

/** A browser destination stays a real anchor while native shells open it outside the application. */
export function ExternalLink({ href, appearance = "text", size, variant = "secondary", onOpen, onFailure, isPending = false, children, className = "", ...props }: Props): React.ReactElement {
  const t = useT();
  const focus = useFocusVisibleProps();
  const action = useExternalLinkAction({ href, onOpen, onFailure, isPending });
  if (!action.address) return <span>{children}</span>;
  const label = <>{children}<ExternalLinkIcon className={appearance === "button" ? "size-3.5" : "ms-1 size-[0.85em]"} /></>;
  return (
    <a
      {...props}
      {...focus}
      href={action.address}
      target="_blank"
      rel="noopener noreferrer"
      aria-description={props["aria-description"] ?? t("common.opensExternally")}
      aria-disabled={action.busy || undefined}
      aria-busy={action.busy || undefined}
      data-external-link=""
      className={`outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus ${appearance === "button" ? `${buttonVariants({ variant, size: size === "xs" ? "sm" : size })} ${size === "xs" ? "app-button--xs" : ""}` : "rounded-sm break-words text-(--tint-accent-fg) underline underline-offset-2"} ${className}`}
      onClick={action.onClick}
      onAuxClick={action.onAuxClick}
    >
      {label}
    </a>
  );
}
