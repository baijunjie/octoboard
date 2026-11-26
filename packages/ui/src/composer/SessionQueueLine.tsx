import React, { useState } from "react";

import { StatusAnnouncer } from "../components/announce/StatusAnnouncer";
import { usePopoverOwnerProps } from "../components/hint/popoverOwner";
import { FadedScroller } from "../components/scroll/scrollFade";
import { FadeOverflow } from "../components/text/FadeOverflow";
import { useT } from "../i18n/react";
import { Button, liveAnnouncerProps, Popover, PopoverContent } from "../primitives";
import { useDaemon } from "../store";
import { TerminalNoticeBar } from "../terminal/TerminalNoticeBar";
import { useAnnouncedOnRise } from "../terminal/useAnnouncedOnRise";
import { sessionComposerEnabled } from "./composerSetting";
import { QueuedMessageList } from "./QueuedMessageList";
import { useMessageQueue, useQueueOffered } from "./messageQueue";

/** Whether the compact queue line shows over `session`'s terminal: the composer is off in this window and the session has
 * messages queued (from another window, say), so they never go out unseen. */
export function useQueueLineShown(session: string | undefined): boolean {
  const enabled = sessionComposerEnabled.useValue();
  const offered = useQueueOffered();
  const count = useMessageQueue(session).length;
  return !enabled && offered && count > 0;
}

/**
 * The compact line over the top of a session's terminal while the composer is off in this window and the session has
 * messages queued: how many, and a button that lists them, each with Send now and Withdraw. A stop of the F6 cycle
 * (`queue`) while it shows; `held` hands focus on when the line goes with focus in it, as the terminal's other bars do.
 * Its controls take no focus on a press, as everything over the terminal does.
 */
export function SessionQueueLine({ session, held, deferred = false }: {
  session: string;
  /** The terminal's announcements are held (the pane out of reach, or a pending cover over it): the line's sentence is
   * held with them. */
  deferred?: boolean;
  held: { onFocus: (event: React.FocusEvent) => void; onBlur: (event: React.FocusEvent) => void };
}): React.ReactElement {
  const t = useT();
  const { store, request } = useDaemon();
  const shown = useQueueLineShown(session);
  const messages = useMessageQueue(session);
  const owner = usePopoverOwnerProps();
  const count = messages.length;
  // The bar's sentence is hidden from assistive technology, as every bar over the terminal's is: the line coming up under
  // the open session is said from a region that stays mounted while the bar comes and goes, after an attended wait,
  // held while the terminal's announcements are, and dropped when the session changes; a session opened with messages
  // already queued is not announced apart from it.
  const due = useAnnouncedOnRise(session, shown, t("queue.count", { count }), deferred);
  // The sentence as it was when it came due, which carries the count at that moment (a change during the attended wait
  // included); a later change while the line shows is not said. Let go once nothing is due, for the next rise.
  const [delivered, setDelivered] = useState<string>();
  if (due === undefined && delivered !== undefined) setDelivered(undefined);
  if (due !== undefined && delivered === undefined) setDelivered(due);
  const said = due === undefined ? undefined : (delivered ?? due);
  // A live announcer, as the terminal's attended regions are: the write may come with an unrelated modal open. It takes no
  // layout box, so it adds no gap among the bars over the terminal.
  const announcer = <div {...liveAnnouncerProps()} className="contents"><StatusAnnouncer text={said} clearAfterRead /></div>;
  if (!shown) return announcer;
  return (
    <>
      {announcer}
      <div data-region="queue" {...held} className="contents">
        <TerminalNoticeBar
          compact
          sentence={<FadeOverflow as="span" titleWhenClipped={t("queue.count", { count })} className="min-w-0 flex-1">{t("queue.count", { count })}</FadeOverflow>}
          controls={(
            <Popover>
              <Button size="xs" variant="secondary" focusOnPress={false} label={t("queue.view.label", { count })}>
                {t("queue.view")}
              </Button>
              <PopoverContent
                placement="bottom end"
                label={t("queue.label")}
                className="control-fills w-[min(28rem,calc(100vw-1.5rem))]"
                dialogClassName="p-1"
                {...owner}
              >
                <FadedScroller className="max-h-[min(24rem,60vh)]">
                  <QueuedMessageList
                    session={session}
                    messages={messages}
                    context={{ store, request, t }}
                    fallback={() => null}
                    label={t("queue.label")}
                  />
                </FadedScroller>
              </PopoverContent>
            </Popover>
          )}
        />
      </div>
    </>
  );
}
