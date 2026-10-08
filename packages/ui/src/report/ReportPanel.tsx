import { Button, Chip } from "@heroui/react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { useCurrentLanguage, useT } from "../i18n/react";
import { drawerClass, PANE_ID, PeekHotZone } from "../layout/paneOverlay";
import type { PanePeek } from "../layout/usePaneToggles";
import type { Page } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import {
  composePageDocument,
  ESCAPE_MESSAGE_SOURCE,
  REGION_MESSAGE_SOURCE,
  SUBMIT_MESSAGE_SOURCE,
  submissionFromEntries,
} from "./pageDocument";

/**
 * A console session's report panel: shown only while a console session is selected, which is what
 * makes its pages visible at all (there is nowhere else to show them). Lists pages on mount and on
 * every snapshot — the daemon never replays a missed `page_created` on its own (see the `page_list`
 * row under "Daemon to client" in `apps/daemon/PROTOCOL.md`), so re-listing is the only way to recover
 * from one. Switching to another console session mounting a fresh instance, so that its position is
 * not carried over from the previous one, is the call site's concern, not this component's.
 *
 * Below the `docked` breakpoint this renders as a closed-by-default overlay instead of a row
 * sibling (`open`, owned by `usePaneToggles` alongside the sidebar's own overlay state) — never
 * unmounted by closing it or by hiding the docked panel, since that would lose the `list_pages`
 * state above and re-request it on every reopen.
 */
export function ReportPanel({
  consoleSessionId,
  open,
  reportWidth,
  peek,
  onEscape,
  onCycleRegion,
}: {
  consoleSessionId: string;
  open: boolean;
  /** The user's chosen width (`usePaneWidth`) for the docked and the floating forms; the drawer
   * below the breakpoint ignores it. */
  reportWidth: number;
  /** The hover reveal of the panel while the user has hidden the docked one from the top bar (which
   * has no effect below the breakpoint, where `open` decides): the same panel, kept as a fixed
   * overlay that floats in over the terminal, with its shadow and rounded edge. `undefined` while
   * the docked panel is shown. Hiding never unmounts the panel, for the same reason as `open`. */
  peek?: PanePeek;
  /** Escape was pressed inside the page's frame, where this document never sees the key. The
   * owner closes whichever overlay is open, as Escape does elsewhere. */
  onEscape: () => void;
  /** F6 (`backward` with Shift) was pressed inside the page's frame; the owner moves focus to the
   * neighbouring region of the window. */
  onCycleRegion: (backward: boolean) => void;
}): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { request, toastError } = useDaemon();
  const sessionPages = useDaemonStore((s) => s.pages.get(consoleSessionId));
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);

  // The user's position, held as the id of the page they paged to rather than an index —
  // `undefined` means "following the newest". An id survives the list changing size or shape
  // (a reconnect's re-list, or a `page_created` appending) because it is looked up fresh on every
  // render instead of being carried across one as a stale index would be.
  const [anchorId, setAnchorId] = useState<string>();

  // `snapshotEpoch` (rather than `connectionState` alone) is what makes lag recovery re-list too:
  // the daemon answers a lagged broadcast receiver with a fresh `snapshot` on the same socket, so
  // the connection never goes through a state change of its own.
  useEffect(() => {
    if (connectionState !== "open") return;
    // Guards against a slow reply for a console session the user has since switched away from (and
    // possibly back to) landing a now-stale error after the fact.
    let stale = false;
    request({ type: "list_pages", console_session: consoleSessionId }).catch((err) => {
      if (!stale) toastError((err as Error).message);
    });
    return () => {
      stale = true;
    };
  }, [consoleSessionId, connectionState, snapshotEpoch, request, toastError]);

  const handleSubmit = useCallback(
    (page: Page, data: unknown) => {
      // The daemon refuses `submit_page` for any page that is not this console session's newest (see
      // "Client to daemon" in `apps/daemon/PROTOCOL.md`), so a stale submission in flight from a page
      // the user has since paged away from is caught there, not here — this just forwards it and
      // reports whatever comes back.
      //
      // A submission is delivered to the console session that pushed the page, which is this
      // panel's, so that is what the daemon's refusals are about ("This session is not running.",
      // "This session is waiting for you."): naming it keeps the user from reading the message as being
      // about whatever session they are looking at.
      request({ type: "submit_page", page: page.id, data }).catch((err) => {
        toastError((err as Error).message, consoleSessionId);
      });
    },
    [request, toastError, consoleSessionId],
  );

  // The panel keeps its place in the row while the first `list_pages` is in flight: dropping out
  // and back would resize the terminal pane, a real SIGWINCH to the agent, on every console
  // session switch.
  // Below the `docked` breakpoint "its place" is a fixed overlay instead, so resizing the
  // terminal never comes up there in the first place — `open` only ever slides it on and off
  // screen, never changes whether it is mounted.
  //
  // `drawerClass` fits the drawer between `--top-chrome-height` and `--bottom-chrome-height`,
  // leaving the top bar (and its report toggle) and the connection banner visible while it is open,
  // and puts it back as a plain row sibling at or above the breakpoint — see that function's own
  // comment for the geometry.
  //
  // The drawer below the breakpoint is a fixed 420px, capped at 92vw. The docked panel is
  // `flex: 0 1` at `--report-width`, the chosen width already held back to what the row affords;
  // the shrink and the 300px floor are only a safety net. With the docked panel hidden,
  // `drawerClass` instead keeps it the overlay at every width, floating in while `peek` is
  // active at `--report-width` (still capped at 92vw); `overflow-hidden` clips the iframe to the
  // rounded edge, and the surface background keeps the empty states from showing the terminal
  // through.
  const overlay = peek
    ? `docked:w-(--report-width) docked:rounded-s-xl docked:overflow-hidden docked:bg-surface ${drawerClass("end", "floating", open, peek.active)}`
    : `docked:w-auto docked:max-w-none docked:min-w-[300px] docked:flex-[0_1_var(--report-width)] ${drawerClass("end", "drawer", open)}`;
  const panelClass = `flex min-h-0 flex-col border-s border-separator w-[420px] max-w-[92vw] ${overlay}`;

  // `data-escape-scope`: one of the origins `usePaneToggles`'s capture-phase Escape listener closes a
  // drawer for, on every branch below since any of them can be what is on screen while open.
  //
  // The pointer handlers sit on the panel, not the iframe: a pointer inside the sandboxed frame
  // sends nothing to this document. Entering the frame therefore must not count as leaving, and
  // the frame's own enter and leave (reported by this document for the iframe element) stand in
  // for the panel's across that boundary, `onPointerMove` for coming back out onto the panel's
  // chrome, where the panel itself never saw the pointer leave.
  const pane = {
    id: PANE_ID.report,
    "data-pane": "report",
    "data-region": "report",
    "data-escape-scope": true,
    style: { "--report-width": `${reportWidth}px` } as React.CSSProperties,
    onPointerEnter: peek?.keep,
    onPointerMove: peek?.keep,
    onPointerLeave: peek?.leave,
  };
  const hotZone = peek && <PeekHotZone side="end" peek={peek} />;

  if (sessionPages === undefined) {
    return (
      <>
        <div {...pane} className={panelClass} />
        {hotZone}
      </>
    );
  }

  if (sessionPages.length === 0) {
    return (
      <>
        <div {...pane} className={`${panelClass} items-center justify-center text-sm text-muted`}>
          {t("report.empty")}
        </div>
        {hotZone}
      </>
    );
  }

  // Look the anchor up fresh: an id either still names a page in the current list or it does not,
  // and falling back to the newest on a miss is the "re-arm following" behaviour `goTo` relies on.
  const anchorIndex = anchorId !== undefined ? sessionPages.findIndex((p) => p.id === anchorId) : -1;
  const displayIndex = anchorIndex === -1 ? sessionPages.length - 1 : anchorIndex;
  const page = sessionPages[displayIndex];
  const isHistory = displayIndex !== sessionPages.length - 1;

  const goTo = (index: number) => {
    // Clearing the anchor on the newest page makes "following" its own state again: the next
    // `page_created` then needs no special-casing to keep the view on the new newest page.
    setAnchorId(index === sessionPages.length - 1 ? undefined : sessionPages[index].id);
  };

  return (
    <>
      <div {...pane} className={panelClass}>
        {/* A previous / next pager with "n / m", hand-assembled from buttons: HeroUI's `Pagination`
            is a list of numbered pages, which neither reads as nor behaves like this. */}
        <div className="flex h-10 shrink-0 items-center gap-2 border-b border-separator bg-surface px-3 text-xs text-muted">
          <TitledControl title={t("report.previous")}>
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label={t("report.previous")}
              preventFocusOnPress
              isDisabled={displayIndex === 0}
              onPress={() => goTo(displayIndex - 1)}
            >
              <ChevronLeft aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            </Button>
          </TitledControl>
          <span className="whitespace-nowrap">
            {t("report.position", { index: displayIndex + 1, total: sessionPages.length })}
          </span>
          <TitledControl title={t("report.next")}>
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              aria-label={t("report.next")}
              preventFocusOnPress
              isDisabled={displayIndex === sessionPages.length - 1}
              onPress={() => goTo(displayIndex + 1)}
            >
              <ChevronRight aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            </Button>
          </TitledControl>
          {/* The timestamp's width is whatever the user's locale makes of it, so it is the element
              that gives way, rather than pushing the badge off the panel's edge when narrow. */}
          <FadeOverflow as="span" className="ms-auto min-w-0">
            {new Date(page.created_at).toLocaleString(language)}
          </FadeOverflow>
          {isHistory && (
            <Chip size="sm" variant="soft" color="warning" className="shrink-0">
              {t("report.readOnly")}
            </Chip>
          )}
        </div>
        <PageFrame
          key={page.id}
          page={page}
          isHistory={isHistory}
          onSubmit={handleSubmit}
          onEscape={onEscape}
          onCycleRegion={onCycleRegion}
          onPointerEnter={peek?.keep}
          onPointerLeave={peek?.leave}
        />
      </div>
      {hotZone}
    </>
  );
}

/** A fresh unguessable value for one frame's script nonce. */
function newNonce(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

/**
 * The sandboxed render of one page. Keyed by `page.id` at the call site so React remounts this
 * (and so the iframe) rather than mutating it across a page change, which also gives each page its
 * own script nonce.
 */
function PageFrame({
  page,
  isHistory,
  onSubmit,
  onEscape,
  onCycleRegion,
  onPointerEnter,
  onPointerLeave,
}: {
  page: Page;
  isHistory: boolean;
  onSubmit: (page: Page, data: unknown) => void;
  onEscape: () => void;
  onCycleRegion: (backward: boolean) => void;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
}): React.ReactElement {
  const t = useT();
  const iframeRef = useRef<HTMLIFrameElement>(null);
  // State rather than a memo: React may drop a memo's cache, and a new nonce reloads the frame,
  // losing whatever the user has typed into the page.
  const [nonce] = useState(newNonce);
  const srcDoc = useMemo(
    () => composePageDocument(page.html, isHistory, nonce),
    [page.html, isHistory, nonce],
  );

  useEffect(() => {
    const handleMessage = (event: MessageEvent) => {
      // `sandbox="allow-scripts allow-forms"` without `allow-same-origin` gives the frame an opaque origin, so
      // its messages arrive with `event.origin === "null"` — a string every opaque frame shares,
      // not something that identifies this one. The only reliable check is that the message came
      // from this iframe's own window.
      if (event.source !== iframeRef.current?.contentWindow) return;
      const data = event.data as { source?: unknown } | null;
      if (!data) return;
      // The relay carries nothing from the page, so a page forging it can only ask for what the
      // user's own Escape would do.
      if (data.source === ESCAPE_MESSAGE_SOURCE) return onEscape();
      // Likewise only moves focus, as the user's own F6 would, and only while focus is in the frame:
      // a page posting it at any other time would pull focus off wherever the user is.
      if (data.source === REGION_MESSAGE_SOURCE) {
        if (document.activeElement !== iframeRef.current) return;
        return onCycleRegion((data as { backward?: unknown }).backward === true);
      }
      // A history page's bridge already posts nothing, and the daemon refuses a `submit_page` for
      // one regardless (it is the enforcement point), but a forged message would surface that
      // refusal as an error toast for an action the user never took, so a history page's messages
      // are not forwarded.
      if (isHistory || data.source !== SUBMIT_MESSAGE_SOURCE) return;
      const submission = submissionFromEntries((data as { entries?: unknown }).entries);
      if (submission) onSubmit(page, submission);
    };
    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, [page, isHistory, onSubmit, onEscape, onCycleRegion]);

  return (
    <iframe
      ref={iframeRef}
      className="min-h-0 flex-1 border-0 bg-white"
      title={t("report.frameTitle")}
      sandbox="allow-scripts allow-forms"
      onPointerEnter={onPointerEnter}
      onPointerLeave={onPointerLeave}
      srcDoc={srcDoc}
    />
  );
}
