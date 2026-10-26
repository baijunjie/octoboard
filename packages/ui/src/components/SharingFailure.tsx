import React, { useId } from "react";
import { useShallow } from "zustand/react/shallow";

import { sharingStateKey, sharingCauseGroups, sharingCauseKey, sharingFixKey, sharingReasonKey, sharingItemLabel, type SharingFailureItem } from "../daemonMessage";
import { joinNodes } from "../i18n/joinPhrases";
import { Message, useCurrentLanguage, useT } from "../i18n/react";
import type { SharingEntryCause } from "../protocol";
import { useErrorRecords } from "../store";
import { DiagnosticBlock } from "./DiagnosticBlock";
import { InlineReference } from "./InlineReference";

/** The names of the sessions with these ids, read through a selector that changes only when one of those names does. */
function useSessionTitles(items: SharingFailureItem[]): (id: string) => string | undefined {
  const ids = [...new Set(items.flatMap((item) => item.sessions))];
  const titles = useErrorRecords(useShallow((s) => ids.map((id) => s.sessions.get(id)?.title)));
  return (id) => titles[ids.indexOf(id)];
}

/** A reason a sharing change did not finish with what to do about it after it. The catalog joins the two, since what
 * stands between two sentences is the language's to say. */
function SharingReason({ reason, fix }: { reason: React.ReactNode; fix?: ReturnType<typeof sharingFixKey> }): React.ReactElement {
  return fix ? <Message id="daemon.sharing.withFix" params={{ cause: reason, fix: <Message id={fix} /> }} /> : <>{reason}</>;
}

/** Why a change failed as a whole, with no entry to name, and what to do about it: the cause's own line, or, for a
 * cause with none (`other`), pointing at the raw reason in Details when there is one. Nothing when there is neither. */
export function SharingFailureReason({ cause, detail, withFix = true }: { cause?: string; detail?: string; withFix?: boolean }): React.ReactElement | null {
  const reasonKey = sharingReasonKey(cause);
  if (reasonKey) return <SharingReason reason={<Message id={reasonKey} />} fix={withFix ? sharingFixKey(cause) : undefined} />;
  return detail ? <Message id="daemon.sharing.fix.other" /> : null;
}

/** One cause of a sharing failure as a line naming the entries it hit ("You do not have permission to change
 * skills/ and 2 more."), a conversation by the name of a session that records it, and, unless `withFix` is false,
 * going on with what to do about it (`rawBelow` as for `sharingFixKey`). */
export function SharingCauseLine({ group, withFix = true, rawBelow = false }: { group: { cause: SharingEntryCause; items: SharingFailureItem[] }; withFix?: boolean; rawBelow?: boolean }): React.ReactElement {
  const title = useSessionTitles(group.items);
  const chips = group.items.map((item, at) => {
    const label = sharingItemLabel(item, title);
    return <InlineReference key={at} dir={label.session ? "auto" : "ltr"} monospace={!label.session} wrap>{label.text}</InlineReference>;
  });
  const phrase = chips.length === 1 ? chips[0] : <Message id="daemon.sharing.items" params={{ first: chips[0], count: chips.length - 1 }} />;
  return <SharingReason reason={<Message id={sharingCauseKey(group.cause)} params={{ items: phrase, count: chips.length } as never} />} fix={withFix ? sharingFixKey(group.cause, rawBelow) : undefined} />;
}

/** What a sharing failure left, said short: one line per cause naming the entries it hit ("You do not have permission
 * to change skills/ and 2 more."), a conversation by the name of a session that records it. Each line goes on with
 * what to do about its cause. It is the further explanation above the raw entries, at the top of Details or, with
 * `rawBelow`, of a popover that holds them too. */
export function SharingCauseSummary({ items, rawBelow = false }: { items: SharingFailureItem[]; rawBelow?: boolean }): React.ReactElement {
  return (
    <span className="flex min-w-0 flex-col gap-1">
      {sharingCauseGroups(items).map((group) => <span key={group.cause}><SharingCauseLine group={group} rawBelow={rawBelow} /></span>)}
    </span>
  );
}

/** The raw side of a sharing failure: each entry with what it was left as and what the system said of it, or, with
 * no entry, the one reason the change could not start, for a place that is itself the details (an alert's popover,
 * the error alert's "Error details"). Nothing at all when there is nothing raw. */
export function SharingDetails({ items, detail }: { items: SharingFailureItem[]; detail?: string }): React.ReactElement | null {
  const t = useT();
  const language = useCurrentLanguage();
  const title = useSessionTitles(items);
  const base = useId();
  if (items.length === 0 && !detail) return null;
  if (items.length === 0) return <DiagnosticBlock>{detail!}</DiagnosticBlock>;
  return (
    <ul className="ms-4 flex min-w-0 list-disc flex-col gap-1">
      {items.map((item, index) => {
        const state = t(sharingStateKey(item));
        const nameId = `${base}-${index}`;
        const name = <InlineReference dir="ltr" monospace wrap>{item.item}</InlineReference>;
        return (
          <li key={index}>
            <span id={nameId}>{item.sessions.length === 0
              ? <Message id="daemon.sharing.item.summary" params={{ item: name, state }} />
              : <Message id="daemon.sharing.conversation.summary" params={{
                item: name,
                sessions: joinNodes(language, item.sessions, (id, key) => <InlineReference key={key} dir={title(id) !== undefined ? "auto" : "ltr"} monospace={title(id) === undefined} wrap>{title(id) ?? id}</InlineReference>),
                state,
              }} />}</span>
            {item.detail && <div className="mt-1"><DiagnosticBlock labelledBy={nameId}>{item.detail}</DiagnosticBlock></div>}
          </li>
        );
      })}
    </ul>
  );
}
