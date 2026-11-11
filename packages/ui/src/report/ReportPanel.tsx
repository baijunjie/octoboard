import { errorPresentation, type ErrorPresentation } from "../errors/errorPresentation";
import { FileText } from "lucide-react";
import React, { useEffect, useId, useMemo, useState } from "react";

import { PagerButton } from "../components/controls/PagerButton";
import { PaneFailure } from "../components/alerts/PaneFailure";
import { raiseError } from "../components/alerts/raiseError";
import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { RetryStatusMark } from "../components/status/RetryStatusMark";
import { StatusBadge } from "../components/status/StatusBadge";
import { HandoffOnUnmount } from "../components/focus/HandoffOnUnmount";
import { handFocusToRegionStart } from "../components/focus/modalFocus";
import { PanelHeader } from "../components/panel/PanelHeader";
import { CenteredEmptyPanel } from "../components/panel/EmptyPanel";
import { FadedScroller, useFadeRegion } from "../components/scroll/scrollFade";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { CodedError } from "../errors/codedError";
import { useCurrentLanguage, useT } from "../i18n/react";
import type { Page, SubmittedValue } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";
import { listPages } from "./listPages";
import { formatPageTime } from "./pageTime";
import type { Pressed } from "./page/PageFormContext";
import { defaultValues, type FormValues } from "./page/formModel";
import { PageView } from "./page/PageView";

/**
 * A session's report panel: the Report panel of the right pane (`panels/panelRegistry.tsx`) while
 * a console session owns the pane (see `layout/asideOwner.ts`) or a project session with pages is
 * selected, which is what makes its pages visible at all. Lists pages (`listPages.ts`) on mount and on
 * every snapshot — the daemon never replays a missed `page_created` on its own (see the `page_list`
 * row under "Daemon to client" in `apps/daemon/PROTOCOL.md`), so re-listing is the only way to recover
 * from one. Switching to another session mounting a fresh instance, so that its position is
 * not carried over from the previous one, is the call site's concern, not this component's.
 */
export function ReportPanel({ sessionId }: { sessionId: string }): React.ReactElement {
  const t = useT();
  const { request, store } = useDaemon();
  const language = useCurrentLanguage();
  const sessionPages = useDaemonStore((s) => s.pages.get(sessionId));
  const connectionState = useDaemonStore((s) => s.connectionState);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);

  // The user's position, held as the id of the page they paged to rather than an index —
  // `undefined` means "following the newest". An id survives the list changing size or shape
  // (a reconnect's re-list, or a `page_created` appending) because it is looked up fresh on every
  // render instead of being carried across one as a stale index would be.
  const [anchorId, setAnchorId] = useState<string>();
  // Why the last listing failed, until one succeeds; `attempt` asks again. With no pages to show it is
  // the state of the panel, a whole-card failure that announces itself; with pages it is a mark at
  // the title (`showsPages`), which the panel's status region says.
  const [listFailure, setListFailure] = useState<ErrorPresentation>();
  // Counts the failures, so a whole-card failure that repeats in the same words is announced again.
  const [failures, setFailures] = useState(0);
  // The failure (by its count) the whole card announced, which a mark that replaces it must not say again.
  const [spokenAsCard, setSpokenAsCard] = useState(0);
  const markId = useId();
  const showsPages = sessionPages !== undefined && sessionPages.length > 0;
  const [attempt, setAttempt] = useState(0);
  // Whether Try again's listing is out: its button stays where it is, unavailable, rather than
  // taking a second press, and the failure stays shown until the listing answers.
  const [retrying, setRetrying] = useState(false);

  // `snapshotEpoch` (rather than `connectionState` alone) is what makes lag recovery re-list too:
  // the daemon answers a lagged broadcast receiver with a fresh `snapshot` on the same socket, so
  // the connection never goes through a state change of its own.
  useEffect(() => {
    if (connectionState !== "open") {
      setRetrying(false);
      return;
    }
    // Guards against a slow reply for a session the user has since switched away from (and
    // possibly back to) landing a now-stale error after the fact.
    let stale = false;
    listPages({ request, store }, sessionId, snapshotEpoch).then(
      () => {
        if (stale) return;
        setListFailure(undefined);
        setRetrying(false);
      },
      (err: unknown) => {
        if (stale) return;
        setListFailure(errorPresentation(err));
        setFailures((n) => n + 1);
        setRetrying(false);
      },
    );
    return () => {
      stale = true;
    };
  }, [sessionId, connectionState, snapshotEpoch, attempt, request, store]);

  // The submission whose `submit_page` is out, if any: the page's own id, as the user may have paged away by the reply,
  // and the key of the button that was pressed (`submitterKeys`), which unlike the component survives the page being
  // listed anew, and tells a button with no name from another.
  const [pending, setPending] = useState<{ page: string; pressed: string }>();
  // What this window sent and the daemon accepted, by page, for the moments before the daemon's own `page_submission`
  // reaches the store: the reply and the event are not ordered against each other, and the form must not unlock between
  // them.
  const [acceptedById, setAcceptedById] = useState<Readonly<Record<string, Record<string, SubmittedValue>>>>({});

  // What the user has entered into each page's form, by page id, so that it outlives the page's view: paging away
  // unmounts it, and a failed submission unlocks the form on a page the user may not be looking at. A page
  // that has left the list loses its draft and what was accepted for it.
  const [drafts, setDrafts] = useState<Readonly<Record<string, FormValues>>>({});
  useEffect(() => {
    if (sessionPages === undefined) return;
    function listed<T>(held: Readonly<Record<string, T>>): Readonly<Record<string, T>> {
      const kept = Object.keys(held).filter((id) => sessionPages!.some((p) => p.id === id));
      return kept.length === Object.keys(held).length ? held : Object.fromEntries(kept.map((id) => [id, held[id]]));
    }
    setDrafts(listed);
    setAcceptedById(listed);
  }, [sessionPages]);

  const submit = (id: string, data: Record<string, SubmittedValue>, pressed: Pressed) => {
    const done = () => setPending((current) => (current?.page === id ? undefined : current));
    const accept = () => setAcceptedById((held) => ({ ...held, [id]: data }));
    setPending({ page: id, pressed: pressed.key });
    // The daemon refuses `submit_page` for any page that is not this session's newest or that is already submitted
    // (see "Client to daemon" in `apps/daemon/PROTOCOL.md`), so a stale submission in flight from a page the user has
    // since paged away from, or one raced by another window, is caught there, not here: this forwards it and reports
    // whatever comes back.
    //
    // A submission is delivered to the session that pushed the page, which is this panel's, so that is what the daemon's
    // refusals are about ("This session is not running.", "This session is waiting for you."): naming it keeps the user
    // from reading the message as being about whatever session they are looking at.
    request({ type: "submit_page", page: id, data }).then(
      () => {
        accept();
        done();
      },
      (err: unknown) => {
        // The message may be in the session: the daemon keeps the page submitted, and so does this.
        if (err instanceof CodedError && err.code === "message_acceptance_unconfirmed") accept();
        done();
        raiseError(err, { title: t("report.submitFailedTitle"), session: sessionId });
      },
    );
  };

  if (listFailure !== undefined && !showsPages && spokenAsCard !== failures) setSpokenAsCard(failures);

  const retryList = () => {
    setRetrying(true);
    setAttempt((n) => n + 1);
  };

  // Pages already here stay usable, so the failure to refresh them is a warning at the title, not an
  // Alert above them. A failure with no pages is the whole-card one, which announces itself.
  const listingFailureMark = listFailure !== undefined && showsPages && (
    <>
      {/* The mark goes as the listing answers; focus it or its popover held goes to the header's first control. */}
      <HandoffOnUnmount from={`[data-status-mark-id="${markId}"]`} to={() => handFocusToRegionStart("panel-report")} />
      <RetryStatusMark tone="warning" markId={markId} label={t("report.listFailed")} error={listFailure} retrying={retrying} onRetry={retryList}>
        <p>{t("report.listFailed.sentence")}</p>
      </RetryStatusMark>
    </>
  );
  // The panel keeps its place while the first `list_pages` is in flight: dropping out and back
  // would resize the terminal pane, a real SIGWINCH to the agent, on every session switch.
  const renderPanel = (): React.ReactElement => {
    // A fragment like the states after it, so the header, and focus on its Close button, survives the
    // list arriving.
    if (sessionPages === undefined && listFailure === undefined) return <><PanelHeader title={t("panel.report.name")} /></>;
    if (sessionPages === undefined || (sessionPages.length === 0 && listFailure !== undefined)) {
      return (
        <>
          <PanelHeader title={t("panel.report.name")} />
          {listFailure && (
            <FailureGoes className="contents">
              <PaneFailure title={t("report.listFailed")} message={listFailure} announce attempt={failures} pending={retrying} onRetry={retryList} />
            </FailureGoes>
          )}
        </>
      );
    }

    if (sessionPages.length === 0) {
      return (
        <>
          <PanelHeader title={t("panel.report.name")} />
          <CenteredEmptyPanel icon={FileText} message={t("report.empty")} />
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

    const submission = page.submission ?? acceptedById[page.id] ?? null;
    const submitting = pending?.page === page.id;

    return (
      <>
        {/* A previous / next pager with "n / m", hand-assembled from buttons: HeroUI's `Pagination` is a list of
          numbered pages, which neither reads as nor behaves like this. After it comes the page's timestamp and the
          states it is in. The timestamp's width is whatever the user's locale makes of it, so it is the element that
          gives way, down to nothing, rather than pushing the pager, the badges or the header's buttons off the panel's
          edge when narrow. */}
        <PanelHeader title={t("panel.report.name")} titleMark={listingFailureMark || undefined} growChildren>
          <div className="flex min-w-0 flex-1 items-center gap-2 text-xs text-app-muted">
            <div className="flex shrink-0 items-center gap-1">
              <PagerButton
                label={t("report.previous")}
                direction="previous"
                isDisabled={displayIndex === 0}
                onPress={() => goTo(displayIndex - 1)}
              />
              {/* The page count is a range written with `/`, which reads left to right whatever the
                interface direction is: mirrored, "1 / 12" would say page 12 of 1. */}
              <span dir="ltr" className="whitespace-nowrap">
                {t("report.position", { index: displayIndex + 1, total: sessionPages.length })}
              </span>
              <PagerButton
                label={t("report.next")}
                direction="next"
                isDisabled={displayIndex === sessionPages.length - 1}
                onPress={() => goTo(displayIndex + 1)}
              />
            </div>
            <FadeOverflow as="span" className="min-w-0 flex-1 contain-inline-size">
              {formatPageTime(page.created_at, language)}
            </FadeOverflow>
            {isHistory && (
              <StatusBadge variant="soft" color="warning">
                {t("report.readOnly")}
              </StatusBadge>
            )}
            {/* Not while this window's submission is out: the daemon marks the page before delivering, so its record
              arrives before the outcome is known. */}
            {submission !== null && !submitting && (
              <StatusBadge variant="soft" color="success">
                {t("report.submitted")}
              </StatusBadge>
            )}
          </div>
        </PanelHeader>
        <PageSection
          key={page.id}
          page={page}
          submission={submission}
          isHistory={isHistory}
          pending={submitting}
          pressed={submitting ? pending.pressed : undefined}
          draft={drafts[page.id]}
          onDraft={(update) => setDrafts((held) => ({ ...held, [page.id]: update(held[page.id] ?? defaultValues(page.content)) }))}
          onSubmit={(data, pressed) => submit(page.id, data, pressed)}
        />
      </>
    );
  };

  // The mark's words are said once through the panel's own status region, mounted through every state
  // above, unless this failure was already announced as the whole card before pages arrived.
  const markSpoken = listFailure !== undefined && showsPages && spokenAsCard !== failures;
  return (
    <>
      <StatusAnnouncer text={markSpoken ? t("report.listFailed") : undefined} />
      {renderPanel()}
    </>
  );
}

/** Holds a failure shown in the panel, and hands focus on when it goes while it holds focus (the
 * listing answered after Try again was pressed from the keyboard): its button goes with it, and
 * the focus would fall to the page. */
function FailureGoes({ className, children }: { className: string; children: React.ReactNode }): React.ReactElement {
  const id = useId();
  return (
    <>
      <HandoffOnUnmount from={`[id="${id}"]`} to={() => handFocusToRegionStart("panel-report")} />
      <div id={id} className={className}>{children}</div>
    </>
  );
}

/**
 * One page's content as a form, in a scroller of its own. Keyed by `page.id` at the call site, so another page starts
 * with a view of its own; the values entered live in the panel's `drafts`, so they outlive it.
 *
 * A page with a `submission` (the daemon's record, which every window has, or what this window sent and the daemon
 * accepted) is read-only and shows what it was submitted with, so a page submitted from another window, or before a
 * reload, looks the same as one submitted here. A history page is read-only too (the daemon refuses a submission from
 * one). While this window's `submit_page` is out (`pending`) the form is locked. A failure the daemon says certainly did
 * not deliver unlocks it, with the values the user entered (the daemon has cleared the mark and told every window); one
 * whose acceptance is unconfirmed leaves the page submitted, as the daemon keeps its mark, and its error says the
 * message may have arrived.
 */
function PageSection({ page, submission, isHistory, pending, pressed, draft, onDraft, onSubmit }: {
  page: Page;
  submission: Page["submission"];
  isHistory: boolean;
  pending: boolean;
  pressed: string | undefined;
  /** What the user has entered, `undefined` while nothing has been. */
  draft: FormValues | undefined;
  onDraft: (update: (held: FormValues) => FormValues) => void;
  onSubmit: (data: Record<string, SubmittedValue>, pressed: Pressed) => void;
}): React.ReactElement {
  const t = useT();
  const pageId = useId();
  const values = useMemo(() => draft ?? defaultValues(page.content), [draft, page.content]);
  const region = useFadeRegion({ label: page.content.title ?? t("panel.report.name") });

  return (
    <>
      <HandoffOnUnmount from={`[id="${pageId}"]`} to={() => handFocusToRegionStart("panel-report")} />
      <FadedScroller id={pageId} region={region} className="min-h-0 flex-1 px-(--pane-inset) pt-2 pb-3">
        <PageView content={page.content} submission={submission} history={isHistory} pending={pending} pressed={pressed} draft={values} onDraft={onDraft} onSubmit={onSubmit} />
      </FadedScroller>
    </>
  );
}
