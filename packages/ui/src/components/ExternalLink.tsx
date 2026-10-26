import { buttonVariants } from "@heroui/styles";
import React, { useRef, useState } from "react";

import { ActionableError } from "../errorRecovery";
import type { Translate } from "../i18n/catalog";
import { useT } from "../i18n/react";
import { useOptionalPlatform } from "../platform/react";
import { reportError } from "../reportError";
import { ExternalLinkIcon } from "./ExternalLinkIcon";
import { useFocusVisibleProps } from "./useFocusVisibleProps";

export { ExternalLinkIcon } from "./ExternalLinkIcon";

type Props = Omit<React.AnchorHTMLAttributes<HTMLAnchorElement>, "href" | "onClick" | "onAuxClick" | "target" | "rel"> & {
  href: string;
  appearance?: "text" | "button";
  onOpen?: (href: string) => void | Promise<void>;
  isPending?: boolean;
};

/** Native menu and terminal links share the anchor's visible failure and manual destination. */
export function reportExternalLinkFailure(t: Translate, href: string, cause: unknown): void {
  reportError(new ActionableError(t("externalLink.unavailable"), "openExternal", { cause }), undefined, undefined, <div className="flex flex-col gap-4"><p>{t("externalLink.unavailable")}</p><p dir="ltr" className="font-mono [overflow-wrap:anywhere]">{href}</p></div>, t("externalLink.unavailableTitle"));
}

/** A browser destination stays a real anchor while native shells open it outside the application. */
export function ExternalLink({ href, appearance = "text", onOpen, isPending = false, children, className = "", ...props }: Props): React.ReactElement {
  const t = useT();
  const platform = useOptionalPlatform();
  const focus = useFocusVisibleProps();
  const opening = useRef(false);
  const [pending, setPending] = useState(false);
  let address: string | undefined;
  try {
    const url = new URL(href);
    if (["http:", "https:", "mailto:"].includes(url.protocol)) address = url.href;
  } catch {
    // Invalid destinations remain readable rather than becoming an executable or relative URL.
  }
  if (!address) return <span>{children}</span>;
  const busy = isPending || pending;
  const intercepted = !!onOpen || !!platform?.links || platform?.kind === "tauri";
  const activate = async () => {
    if (opening.current || busy) return;
    opening.current = true;
    setPending(true);
    try {
      if (onOpen) await onOpen(address);
      else if (platform?.links) await platform.links.open(address);
      else throw new Error(t("externalLink.unavailable"));
    } catch (cause) {
      reportExternalLinkFailure(t, address, cause);
    } finally {
      opening.current = false;
      setPending(false);
    }
  };
  const label = <>{children}<ExternalLinkIcon className={appearance === "button" ? "size-3.5" : "ms-1 size-[0.85em]"} /></>;
  return (
    <a
      {...props}
      {...focus}
      href={address}
      target="_blank"
      rel="noopener noreferrer"
      aria-description={props["aria-description"] ?? t("common.opensExternally")}
      aria-disabled={busy || undefined}
      aria-busy={busy || undefined}
      data-external-link=""
      className={`outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus ${appearance === "button" ? buttonVariants({ variant: "secondary" }) : "rounded-sm break-words text-(--tint-accent-fg) underline underline-offset-2"} ${className}`}
      onClick={(event) => {
        if (busy || opening.current) { event.preventDefault(); return; }
        if (!intercepted) return;
        event.preventDefault();
        void activate();
      }}
      onAuxClick={(event) => {
        if (busy || opening.current) { event.preventDefault(); return; }
        if (!intercepted || event.button !== 1) return;
        event.preventDefault();
        void activate();
      }}
    >
      {label}
    </a>
  );
}
