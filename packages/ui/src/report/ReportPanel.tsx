import { Button } from "../components/Button";
import { errorPresentation } from "../errorPresentation";
import { ChevronLeft, ChevronRight, FileText } from "lucide-react";
import React, { useCallback, useContext, useEffect, useId, useMemo, useRef, useState } from "react";

import { InlineChip } from "../components/InlineChip";
import { HandOffOnUnmount } from "../components/HandOffOnUnmount";
import { focusAfterLoss, focusFirstIn } from "../components/regionFocus";
import { PanelHeader } from "../components/PanelHeader";
import { EmptyPanel } from "../components/EmptyPanel";
import { FadeOverflow } from "../components/FadeOverflow";
import { TitledControl } from "../components/TitledControl";
import { ROW_ICON_BUTTON_CLASS } from "../components/RowControls";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { PanePeek } from "../layout/usePaneToggles";
import { usePlatform } from "../platform/react";
import type { Page } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { PageDocumentTransform } from "./pageDocumentTransform";
import {
  composePageDocument,
  ESCAPE_MESSAGE_SOURCE,
  HISTORY_MESSAGE_SOURCE,
  REGION_MESSAGE_SOURCE,
  SUBMIT_MESSAGE_SOURCE,
  SWITCH_MESSAGE_SOURCE,
  submissionFromEntries,
} from "./pageDocument";

/**
 * A console session's report panel: the Report panel of the right pane (`panels/panelRegistry.tsx`)
 * while that console session owns the pane (see `layout/asideOwner.ts`), which is what makes its
 * pages visible at all. Lists pages on mount and on
 * every snapshot — the daemon never replays a missed `page_created` on its own (see the `page_list`
 * row under "Daemon to client" in `apps/daemon/PROTOCOL.md`), so re-listing is the only way to recover
 * from one. Switching to another console session mounting a fresh instance, so that its position is
 * not carried over from the previous one, is the call site's concern, not this component's.
 */
export function ReportPanel({
  consoleSessionId,
  peek,
  onEscape,
  onCycleRegion,
  onMoveHistory,
  onMoveConsoleSession,
}: {
  consoleSessionId: string;
  /** The floating pane's pointer callbacks, wired onto the page's frame, which sends this document
   * no pointer events of its own; `undefined` while the pane is docked. */
  peek: Pick<PanePeek, "keep" | "leave"> | undefined;
  /** Escape was pressed inside the page's frame, where this document never sees the key. The
   * owner closes whichever overlay is open, as Escape does elsewhere. */
  onEscape: () => void;
  /** F6 (`backward` with Shift) was pressed inside the page's frame; the owner moves focus to the
   * neighbouring region of the window. */
  onCycleRegion: (backward: boolean) => void;
  /** ⌘[ (`backward`) or ⌘] was pressed inside the page's frame; the owner moves through the
   * navigation history, where the window has that shortcut. */
  onMoveHistory: (backward: boolean) => void;
  /** ⌃⇧Tab (`backward`) or ⌃Tab was pressed inside the page's frame; the owner moves to the
   * neighbouring console session, where the window has that shortcut. */
  onMoveConsoleSession: (backward: boolean) => void;
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
      if (!stale) toastError(errorPresentation(err as Error));
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
        toastError(errorPresentation(err as Error), consoleSessionId);
      });
    },
    [request, toastError, consoleSessionId],
  );

  // The panel keeps its place while the first `list_pages` is in flight: dropping out and back
  // would resize the terminal pane, a real SIGWINCH to the agent, on every console session switch.
  if (sessionPages === undefined) return <PanelHeader title={t("panel.report.name")} />;

  if (sessionPages.length === 0) {
    return (
      <>
        <PanelHeader title={t("panel.report.name")} />
        <div className="flex min-h-0 flex-1 flex-col justify-center-safe">
          <EmptyPanel icon={FileText} message={t("report.empty")} />
        </div>
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
      {/* A previous / next pager with "n / m", hand-assembled from buttons: HeroUI's `Pagination`
          is a list of numbered pages, which neither reads as nor behaves like this. */}
      <PanelHeader title={t("panel.report.name")}>
        <div className="flex shrink-0 items-center gap-1 text-xs text-app-muted">
          <TitledControl title={t("report.previous")}>
            <Button
              isIconOnly
              size="xs"
              variant="ghost"
              className={ROW_ICON_BUTTON_CLASS}
              aria-label={t("report.previous")}
              preventFocusOnPress
              isDisabled={displayIndex === 0}
              onPress={() => goTo(displayIndex - 1)}
            >
              <ChevronLeft aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            </Button>
          </TitledControl>
          {/* The page count is a range written with `/`, which reads left to right whatever the
              interface direction is: mirrored, "1 / 12" would say page 12 of 1. */}
          <span dir="ltr" className="whitespace-nowrap">
            {t("report.position", { index: displayIndex + 1, total: sessionPages.length })}
          </span>
          <TitledControl title={t("report.next")}>
            <Button
              isIconOnly
              size="xs"
              variant="ghost"
              className={ROW_ICON_BUTTON_CLASS}
              aria-label={t("report.next")}
              preventFocusOnPress
              isDisabled={displayIndex === sessionPages.length - 1}
              onPress={() => goTo(displayIndex + 1)}
            >
              <ChevronRight aria-hidden="true" className="size-4 rtl:-scale-x-100" />
            </Button>
          </TitledControl>
        </div>
      </PanelHeader>
      {/* The timestamp's width is whatever the user's locale makes of it, so it is the element
          that gives way, rather than pushing the badge off the panel's edge when narrow. */}
      <div className="flex h-8 shrink-0 items-center gap-2 px-(--pane-inset) text-xs text-app-muted">
        <FadeOverflow as="span" className="min-w-0">
          {new Date(page.created_at).toLocaleString(language)}
        </FadeOverflow>
        {isHistory && (
          <InlineChip size="sm" variant="soft" color="warning" className="shrink-0">
            {t("report.readOnly")}
          </InlineChip>
        )}
      </div>
      <PageFrame
        key={page.id}
        page={page}
        isHistory={isHistory}
        onSubmit={handleSubmit}
        onEscape={onEscape}
        onCycleRegion={onCycleRegion}
        onMoveHistory={onMoveHistory}
        onMoveConsoleSession={onMoveConsoleSession}
        onPointerEnter={peek?.keep}
        onPointerLeave={peek?.leave}
      />
    </>
  );
}

/** A fresh unguessable value for one frame's script nonce. */
function newNonce(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

function handFocusToReport(): void {
  // React suspends focus events during removal; wait until the surviving Button can observe
  // its focus and draw its ring. Another hand-off in this commit takes precedence.
  queueMicrotask(() => {
    if (document.activeElement === document.body) focusAfterLoss(() => focusFirstIn("panel-report"));
  });
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
  onMoveHistory,
  onMoveConsoleSession,
  onPointerEnter,
  onPointerLeave,
}: {
  page: Page;
  isHistory: boolean;
  onSubmit: (page: Page, data: unknown) => void;
  onEscape: () => void;
  onCycleRegion: (backward: boolean) => void;
  onMoveHistory: (backward: boolean) => void;
  onMoveConsoleSession: (backward: boolean) => void;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
}): React.ReactElement {
  const t = useT();
  const platform = usePlatform();
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const frameId = useId();
  const transformDocument = useContext(PageDocumentTransform);
  // State rather than a memo: React may drop a memo's cache, and a new nonce reloads the frame,
  // losing whatever the user has typed into the page.
  const [nonce] = useState(newNonce);
  const srcDoc = useMemo(
    () =>
      transformDocument(composePageDocument(page.html, isHistory, nonce, {
        noContextMenu: platform.kind === "tauri",
        relayHistoryKeys: platform.windowChrome !== undefined,
        relaySwitchKeys: platform.windowChrome !== undefined,
      })),
    [page.html, isHistory, nonce, platform.kind, platform.windowChrome, transformDocument],
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
      // Likewise only what the user's own ⌘[ or ⌘] would do, and only while focus is in the frame.
      if (data.source === HISTORY_MESSAGE_SOURCE) {
        if (document.activeElement !== iframeRef.current) return;
        return onMoveHistory((data as { backward?: unknown }).backward === true);
      }
      // Likewise only what the user's own ⌃Tab would do, and only while focus is in the frame.
      if (data.source === SWITCH_MESSAGE_SOURCE) {
        if (document.activeElement !== iframeRef.current) return;
        return onMoveConsoleSession((data as { backward?: unknown }).backward === true);
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
  }, [page, isHistory, onSubmit, onEscape, onCycleRegion, onMoveHistory, onMoveConsoleSession]);

  return (
    <>
      <HandOffOnUnmount from={`[id="${frameId}"]`} to={handFocusToReport} />
      {/* Where a tap's opening of the panel puts focus: on the page, its content. F6, and an
          opening from a mouse or the keyboard, land on the header's first control instead, the
          sandboxed page passing on only some of the window's shortcuts. */}
      <iframe
        id={frameId}
        ref={iframeRef}
        data-touch-landing=""
        className="min-h-0 flex-1 border-0 bg-white"
        title={t("report.frameTitle")}
        sandbox="allow-scripts allow-forms"
        onPointerEnter={onPointerEnter}
        onPointerLeave={onPointerLeave}
        srcDoc={srcDoc}
      />
    </>
  );
}
