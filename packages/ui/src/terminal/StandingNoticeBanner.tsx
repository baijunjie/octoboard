import { X } from "lucide-react";
import React from "react";

import { FailureBody, codedMessageNode } from "../components/alerts/errorParts";
import { RecoveryLink, failureRecoveryAction } from "../components/alerts/ErrorGuidance";
import { usePopoverOwnerProps } from "../components/hint/popoverOwner";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { CodedError } from "../errors/codedError";
import { daemonMessage } from "../errors/daemonMessage";
import { isUncataloguedFailure } from "../errors/uncataloguedFailure";
import { useCurrentLanguage, useT } from "../i18n/react";
import { Button, Popover, PopoverContent } from "../primitives";
import type { Session, StandingNotice } from "../protocol";
import { useDaemonStore } from "../store";
import { shownStandingNotices, standingNoticeKey } from "../store/standingNotices";
import { TerminalNoticeBar } from "./TerminalNoticeBar";

/** A standing notice worded in the window's language, with the names of the records it names as they are now. */
export function useStandingNoticeText(notice: StandingNotice | undefined): string {
  const language = useCurrentLanguage();
  return useDaemonStore((s) => (notice ? daemonMessage(language, notice.code, notice.params, notice.message, s) : ""));
}

/**
 * The banner a session's standing notices (`Session.standing_notices`) are shown as, in its terminal's top slot: a
 * warning bar (`TerminalNoticeBar`'s `warning` kind) holding the newest notice's sentence on one line, then Details (All
 * notices where there are several), which opens the list of them, newest first, each with its sentence, its remedy and
 * the failure it nests where it has them, and its details; then the button that dismisses the notice the bar shows, in
 * this window only. A code the window has no words for is shown by the daemon's own English text. The bar's sentence is
 * hidden from assistive technology: the caller speaks a notice as it arrives (`useAnnouncedOnArrival`).
 *
 * Its two controls are compact and take no focus on a press, as everything over the terminal does; the caller makes the
 * bar a stop of the F6 cycle and hands focus back to the terminal when it goes.
 */
export function StandingNoticeBanner({ notices, onDismiss }: {
  /** The notices to show, in the order raised (the newest last); at least one. */
  notices: readonly StandingNotice[];
  onDismiss: (notice: StandingNotice) => void;
}): React.ReactElement {
  const t = useT();
  const newest = notices[notices.length - 1]!;
  const sentence = useStandingNoticeText(newest);
  // A code this window has no words for is the daemon's English text, whose direction is its own, not the interface's.
  const newestDir = isUncataloguedFailure(newest) ? "auto" : undefined;
  const listLabel = t("terminal.standingNotice.list");
  const owner = usePopoverOwnerProps();
  return (
    <TerminalNoticeBar
      kind="warning"
      compact
      sentence={<FadeOverflow as="span" dir={newestDir} titleWhenClipped={sentence} className="min-w-0 flex-1">{sentence}</FadeOverflow>}
      controls={(
        <>
          <Popover>
            <Button size="xs" variant="secondary" focusOnPress={false}>
              {notices.length > 1 ? t("terminal.standingNotice.all") : t("error.details")}
            </Button>
            <PopoverContent
              placement="bottom end"
              label={listLabel}
              className="control-fills max-w-[min(28rem,calc(100vw-1.5rem))]"
              dialogClassName="flex flex-col gap-3 p-3 text-sm [overflow-wrap:anywhere]"
              {...owner}
            >
              {[...notices].reverse().map((notice) => (
                <StandingNoticeEntry key={standingNoticeKey("", notice)} notice={notice} />
              ))}
            </PopoverContent>
          </Popover>
          <Button
            size="xs"
            variant="ghost"
            iconOnly
            label={t("terminal.standingNotice.dismiss")}
            tooltip={t("terminal.standingNotice.dismiss")}
            focusOnPress={false}
            onPress={() => onDismiss(newest)}
          >
            <X className="size-3.5" />
          </Button>
        </>
      )}
    />
  );
}

/** One notice in the banner's list: its sentence, then its remedy and the failure it nests where its record names them,
 * then its details, as a failure mark's popover lays a failure out (`FailureBody`); the link out of a remedy that has
 * one under them. A code with no words here is its English text alone, in its own direction. */
function StandingNoticeEntry({ notice }: { notice: StandingNotice }): React.ReactElement {
  const t = useT();
  const text = useStandingNoticeText(notice);
  if (isUncataloguedFailure(notice)) {
    return <section className="flex flex-col gap-1 text-foreground"><p dir="auto">{notice.message}</p></section>;
  }
  const presentation = new CodedError(text, notice.code, notice.params);
  // A standing notice's sentence carries its own step (an account read as logged out says to log in in the terminal):
  // it is read in the banner and, by a window that predates the banner, as a toast. So the remedy its record names for
  // the dialogs that refuse on it (`accountNotSignedIn`: choose another account, or log in) is not added here; only the
  // remedy `FailureBody` words, a remedy line of the code's own or of the failure it nests, is.
  return (
    <section className="flex flex-col gap-1 text-app-muted [&>p:first-child]:text-foreground">
      <FailureBody error={presentation} summary={codedMessageNode(notice, text, "summary")} />
      <RecoveryLink action={failureRecoveryAction(t, presentation)} size="xs" />
    </section>
  );
}

/** The standing notices of `session` this window shows (`shownStandingNotices`), none for an archived session, whose
 * history covers its terminal. */
export function useShownStandingNotices(session: Session | undefined): readonly StandingNotice[] {
  const dismissed = useDaemonStore((s) => s.dismissedStandingNotices);
  return session?.status === "archived" ? NO_NOTICES : shownStandingNotices(session, dismissed);
}

const NO_NOTICES: readonly StandingNotice[] = [];
