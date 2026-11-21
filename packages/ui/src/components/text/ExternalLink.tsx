import React, { useRef, useState } from "react";

import { ActionableError } from "../../errors/actionableError";
import type { Translate } from "../../i18n/catalog";
import { Message, useT } from "../../i18n/react";
import { InlineReference } from "../../i18n/InlineReference";
import { isolate } from "../../i18n/isolate";
import { ForwardedLinkOpenError, LinkForwardError } from "../../platform/linkForwardError";
import { useOptionalPlatform } from "../../platform/react";
import { remoteDaemonName } from "../../platform/windowKind";
import { ExternalLinkIcon } from "./ExternalLinkIcon";
import { settle } from "../hooks/settle";
import { raiseError } from "../alerts/raiseError";
import { buttonClassName, Spinner, useBusy, useFocusVisibleProps } from "../../primitives";

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

/** Native menu and terminal links share the anchor's visible failure and manual destination. An address a remote window
 * could not forward is its own alert (`reportForwardFailure`); a browser that failed to open a forwarded address shows
 * that one, since the address as written leads to this computer. */
export function reportExternalLinkFailure(t: Translate, href: string, cause: unknown): void {
  if (cause instanceof LinkForwardError) return reportForwardFailure(t, cause);
  const address = cause instanceof ForwardedLinkOpenError ? cause.address : href;
  raiseError(new ActionableError(t("externalLink.unavailable"), "openExternal", { cause: cause instanceof ForwardedLinkOpenError ? cause.cause : cause }), { title: t("externalLink.unavailableTitle"), content: addressUnavailableContent(t("externalLink.unavailable"), address) });
}

/** The alert for an address a remote window could not open through a forwarded port, worded by what failed with the
 * port and the daemon's name, and with no address to open by hand: as written it leads to this computer. Nothing
 * listening says it all; a server that forbids forwarding and a connection that is down have their remedies; anything
 * else is unexpected, its text under Error details. */
function reportForwardFailure(t: Translate, failure: LinkForwardError): void {
  const title = t("externalLink.unavailableTitle");
  // A port is a number to read digit by digit, with no thousands separator.
  const params = { port: String(failure.port), name: isolate(remoteDaemonName() ?? "") };
  // The port and the daemon's name are chips in the alert's body; the message keeps them as text for the report.
  const chips = { port: <InlineReference monospace dir="ltr">{params.port}</InlineReference>, name: <InlineReference dir="auto" wrap>{remoteDaemonName() ?? ""}</InlineReference> };
  switch (failure.kind) {
    case "nothing_listening":
      return raiseError(new ActionableError(t("remoteForward.nothingListening", params), "none", { cause: failure }), { title, content: <Message id="remoteForward.nothingListening" params={chips} /> });
    case "forwarding_refused":
      return raiseError(new ActionableError(t("remoteForward.refused", params), "forwardRefused", { cause: failure }), { title, content: <Message id="remoteForward.refused" params={chips} /> });
    case "disconnected":
      return raiseError(new ActionableError(t("remoteForward.disconnected", params), "connection", { cause: failure }), { title, content: <Message id="remoteForward.disconnected" params={chips} /> });
    case "failed":
      return raiseError(new Error(failure.message), { title });
  }
}

/** Opens an external address without prescribing how its link is drawn. `target` is the anchor's own: `_blank` only where
 * the browser follows the link itself (a page with no platform adapter, such as a website preview). Where the app opens
 * it, the anchor has none, because the desktop shell's shell plugin opens every web or mail anchor with
 * `target="_blank"` on any click that bubbles to the body, even one whose
 * default is prevented, which would open the address as written a second time and, in a remote window, never forwarded. */
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
    /** Opens the address from a control that is not an anchor (an icon button): the same open, guard against a second
     * press while one is out, and failure alert as a click on the link. */
    activate: () => void activate(),
    target: intercepted ? undefined : "_blank",
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

/** A browser destination stays a real anchor while native shells open it outside the application. In a busy modal it is
 * a link that cannot be followed: no `href`, so neither a click, a middle click nor the keyboard reaches the address,
 * and no tab stop, while it is still read as a link, unavailable. */
export function ExternalLink({ href, appearance = "text", size, variant = "secondary", onOpen, onFailure, isPending = false, children, className = "", ...props }: Props): React.ReactElement {
  const t = useT();
  const focus = useFocusVisibleProps();
  const action = useExternalLinkAction({ href, onOpen, onFailure, isPending });
  const unfollowable = useBusy();
  if (!action.address) return <span>{children}</span>;
  const spinner = appearance === "button" && action.busy && <Spinner size="sm" color="current" className={size === "xs" ? "size-3" : undefined} />;
  const label = <>{spinner}{children}<ExternalLinkIcon className={appearance === "button" ? "size-3.5" : "ms-1 size-[0.85em]"} /></>;
  return (
    <a
      {...props}
      {...focus}
      role={unfollowable ? "link" : undefined}
      aria-disabled={action.busy || unfollowable || undefined}
      // After the disabled mark, so a browser that blurs the link as its `href` goes finds it already marked.
      href={unfollowable ? undefined : action.address}
      target={action.target}
      rel="noopener noreferrer"
      aria-description={props["aria-description"] ?? t("common.opensExternally")}
      aria-busy={action.busy || undefined}
      data-external-link=""
      className={`outline-none ui-keyboard-focus:ring-2 ui-keyboard-focus:ring-focus ${appearance === "button" ? `${buttonClassName({ variant, size })}` : unfollowable ? "rounded-sm break-words cursor-default text-app-muted" : "rounded-sm break-words text-(--tint-accent-fg) underline underline-offset-2"} ${className}`}
      onClick={unfollowable ? undefined : action.onClick}
      onAuxClick={unfollowable ? undefined : action.onAuxClick}
    >
      {label}
    </a>
  );
}
