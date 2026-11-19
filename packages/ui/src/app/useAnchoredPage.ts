import { useEffect, useState } from "react";

import { useStableHandler } from "../components/hooks/useLatest";
import type { Location } from "../navigation/history";

/** What a page shows over: the console and the session that were current when it was opened. */
interface Anchor {
  console: string | undefined;
  session: string | undefined;
}

/**
 * What the Remote devices and Scheduled tasks pages share (`useRemoteDevicesPage`, `useScheduledTasksPage`): a page of
 * its own beside a console's sessions that shows over the console and the session current when it was opened, and is
 * gone as soon as either changes, except that while a form on it holds changes that are not saved (`held`) it
 * re-anchors to what is current instead, so that a change nobody asked for never takes the form away; a page the user
 * leaves asks first, and a confirmed leave calls `leave` or `follow` itself, so `held` never brings a left page back.
 * It also keeps the item last chosen (`preference`, remembered per daemon) and whether a new item's form is open
 * (`adding`), which every way of showing an item ends, and so does the page going away.
 *
 * A page can be anchored to more than the console and the session (`Terms`, which `stamp` makes as a page is opened,
 * or re-anchored when `reanchoring`): it is anchored while `matches` holds of them, and also re-anchors without a held
 * form where `moves` says so. `available` is whether the page can show at all.
 */
export function useAnchoredPage<Terms extends object = Record<never, never>>({ consoleId, sessionId, held, available, preference, stamp, matches, moves }: {
  consoleId: string | undefined;
  sessionId: string | undefined;
  held: boolean;
  available: boolean;
  preference: { get(): string | undefined; set(value: string | undefined): void };
  stamp?: (reanchoring: boolean) => Terms;
  matches?: (page: Terms) => boolean;
  moves?: (page: Terms) => boolean;
}) {
  const terms = (reanchoring: boolean) => stamp?.(reanchoring) ?? ({} as Terms);
  const [page, setPage] = useState<Anchor & Terms>();
  const [chosen, setChosen] = useState(() => preference.get());
  // A new item's form is open in the content area, which the top bar's trail does not name an item for.
  const [adding, setAdding] = useState(false);
  const anchored = page !== undefined && (matches?.(page) ?? true) && page.console === consoleId && page.session === sessionId;
  const reanchors = page !== undefined && !anchored && (held || (moves?.(page) ?? false));
  const shown = available && page !== undefined && (anchored || reanchors);
  useEffect(() => {
    if (!page) return;
    if (!shown) setPage(undefined);
    else if (reanchors) setPage({ console: consoleId, session: sessionId, ...terms(true) });
  });
  // A form being filled in is dropped with the page.
  useEffect(() => {
    if (!shown && adding) setAdding(false);
  }, [shown, adding]);
  const remember = (id: string) => {
    setChosen(id);
    preference.set(id);
  };
  return {
    shown,
    chosen,
    adding,
    startAdd: () => setAdding(true),
    cancelAdd: () => setAdding(false),
    /** Shows an item, remembered, which ends a new item's form. */
    select: (id: string) => {
      setAdding(false);
      remember(id);
    },
    /** Shows the page over what is current now. */
    show: useStableHandler(() => setPage((current) => (shown ? current : { console: consoleId, session: sessionId, ...terms(false) }))),
    leave: () => setPage(undefined),
    /** Sets whether the page shows, and the item it shows, to what a history entry says (`showing`: its id, empty for
     * none, or `undefined` where the page is not shown). */
    follow: (target: Location, showing: string | undefined) => {
      if (showing === undefined) {
        setPage(undefined);
        return;
      }
      setPage({ console: target.console, session: target.session, ...terms(false) });
      setAdding(false);
      if (showing) remember(showing);
    },
  };
}
