import { Spinner } from "@heroui/react";
import { FileQuestion, MessageSquareOff } from "lucide-react";
import React, { Suspense, lazy, useCallback, useId, useRef } from "react";

import { useHasPainted } from "../components/useHasPainted";
import { EmptyPanel } from "../components/EmptyPanel";
import { PaneFailure } from "../components/PaneFailure";
import { PaneTitle } from "../components/PaneTitle";
import { ArchivedSessionMetadata, SessionContextLabel } from "../components/SessionMetadata";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusVisibleProps } from "../components/useFocusVisibleProps";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useFadeRegion, useScrollFade } from "../components/scrollFade";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { HistoryMessage, Session } from "../protocol";
import { useOctoboardTheme } from "../theme";
import { RendererBoundary } from "../viewer/codeRenderer";
import { FencedCode } from "../viewer/FencedCode";
import { useSessionHistory } from "./useSessionHistory";

type ConversationMessage = Omit<HistoryMessage, "speaker"> & { speaker: "user" | "agent" };

// The Markdown renderer, with its library, loads the first time a reply is shown.
const MarkdownDocument = lazy(() => import("../viewer/MarkdownDocument").then((module) => ({ default: module.MarkdownDocument })));

/**
 * What an archived session was about, in place of its terminal: the messages the user sent and
 * the agent's final reply to each turn, with its context size beside the heading. Read-only; the way back to the
 * terminal is reopening the session.
 */
export function SessionHistoryView({ session }: { session: Pick<Session, "id" | "context" | "started_at" | "ended_at"> }): React.ReactElement {
  const t = useT();
  const { history, reload } = useSessionHistory(session.id);
  const painted = useHasPainted();
  const focus = useFocusVisibleProps();
  const rootRef = useRef<HTMLElement>(null);
  // The retry button goes away under focus, into the loading state: it moves to the messages when
  // there are any, else to the view itself.
  const held = useFocusHandoff(() => {
    (rootRef.current?.querySelector<HTMLElement>("[role=region][tabindex]") ?? rootRef.current)?.focus();
  });
  const messages = history.state === "loaded" ? history.messages.filter((message): message is ConversationMessage => message.speaker !== "octoboard") : [];
  const spoken =
    history.state === "loading"
      ? t("history.loading")
      : history.state === "unavailable"
        ? t("history.unavailable.sentence")
        : history.state === "loaded" && messages.length === 0
          ? t("history.empty")
          : undefined;
  return (
    <section
      ref={rootRef}
      tabIndex={-1}
      {...focus}
      onFocus={(event) => { focus.onFocus?.(event); held.onFocus(event); }}
      onBlur={(event) => { focus.onBlur?.(event); held.onBlur(event); }}
      aria-label={t("history.title")}
      className="absolute inset-0 flex flex-col bg-panel outline-none data-focus-visible:ring-2 data-focus-visible:ring-focus data-focus-visible:ring-inset"
    >
      {/* At narrow widths the header wraps its contents so the dates stay readable. */}
      <header className="flex min-h-(--pane-header-height) shrink-0 flex-wrap items-center gap-x-3 gap-y-2 groove-b px-(--pane-inset) py-1.5">
        {/* The context badge follows the heading and never shrinks; the heading clips first. */}
        <div className="flex min-w-24 flex-1 items-center gap-2">
          <PaneTitle heading={t("history.title")} className="min-w-0" />
          <SessionContextLabel context={session.context} />
        </div>
        <ArchivedSessionMetadata session={session} />
      </header>
      <StatusAnnouncer text={spoken} />
      {history.state === "loading" && (
        <div aria-hidden="true" className="flex flex-1 flex-col items-center justify-center-safe gap-3 text-sm text-app-muted">
          <Spinner size="md" color="current" aria-hidden="true" />
          {t("history.loading")}
        </div>
      )}
      {history.state === "failed" && <PaneFailure title={t("history.failed")} message={history.error} announce={painted} onRetry={() => reload()} />}
      {history.state === "unavailable" && (
        // The record's state, not a failed load: reading it again would not change the answer, so it
        // is an empty state rather than a failure. Spoken by the status region above, with its
        // remedy, so it is silent here.
        <div className="flex flex-1 items-center justify-center-safe">
          <EmptyPanel icon={FileQuestion} message={t("history.unavailable")} description={t("history.unavailable.description")} remedy={t("history.unavailable.remedy")} silent />
        </div>
      )}
      {history.state === "loaded" &&
        (messages.length === 0 ? (
          <div className="flex flex-1 items-center justify-center-safe">
            <EmptyPanel icon={MessageSquareOff} message={t("history.empty")} silent />
          </div>
        ) : (
          <MessageList messages={messages} />
        ))}
    </section>
  );
}

/** The messages in a scrolling region of their own, which takes focus so they can be scrolled from
 * the keyboard. The bottom padding clears the floating bar the pane lays over the view. */
function MessageList({ messages }: { messages: ConversationMessage[] }): React.ReactElement {
  const t = useT();
  // Always a region, even when the messages fit: it is the pane's F6 landing and what the keyboard
  // scrolls.
  const region = useFadeRegion({ label: t("history.messages"), always: true });
  const fade = useScrollFade({ region, className: "min-h-0 flex-1 overflow-auto px-(--pane-inset) pt-4 pb-24" });
  return (
    <div className={`flex min-h-0 flex-1 flex-col rounded-xl ${region.focused ? "ring-2 ring-focus ring-inset" : ""}`}>
      <div {...fade}>
        <ol className="flex flex-col gap-4">
          {messages.map((message, index) => (
            <li key={index}>
              <Message message={message} />
            </li>
          ))}
        </ol>
      </div>
    </div>
  );
}

function Message({ message }: { message: ConversationMessage }): React.ReactElement {
  const t = useT();
  const language = useCurrentLanguage();
  const { resolved: theme } = useOctoboardTheme();
  const links = usePlatform().links;
  const onOpenLink = useCallback((url: string) => links?.open(url), [links]);
  const renderCode = useCallback(
    ({ language: fence, text, highlight }: { language?: string; text: string; highlight: boolean }) => (
      <FencedCode language={fence} text={text} theme={theme} highlight={highlight} />
    ),
    [theme],
  );
  const speaker = t(`history.speaker.${message.speaker}`);
  const speakerId = useId();
  const plain = <p dir="auto" className="break-words whitespace-pre-wrap">{message.text}</p>;
  return (
    <article aria-labelledby={speakerId} className="flex flex-col gap-1">
      <div className="flex items-baseline gap-2 text-xs text-app-muted">
        <span id={speakerId} className="font-medium">
          {speaker}
        </span>
        {message.at !== undefined && (
          <time dateTime={new Date(message.at).toISOString()}>{isolate(new Date(message.at).toLocaleString(language))}</time>
        )}
      </div>
      <div data-speaker={message.speaker} className="history-message rounded-xl bg-default px-3 py-2 text-sm leading-6 shadow-(--panel-shadow)">
        {message.speaker === "agent" ? (
          <RendererBoundary resetKey={message.text} fallback={plain}>
            <Suspense fallback={plain}>
              <MarkdownDocument embedded text={message.text} path="" theme={theme} renderCode={renderCode} onOpenLink={links && onOpenLink} />
            </Suspense>
          </RendererBoundary>
        ) : plain}
      </div>
    </article>
  );
}
