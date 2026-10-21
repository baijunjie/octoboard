import { Spinner, Switch } from "@heroui/react";
import { MessageSquareOff, TriangleAlert } from "lucide-react";
import React, { Suspense, lazy, useCallback, useId, useRef, useState } from "react";

import { Button } from "../components/Button";
import { CodeToneContext } from "../components/codeTone";
import { DaemonErrorMessage } from "../components/DaemonErrorMessage";
import { useErrorAcknowledgement } from "../components/useErrorAcknowledgement";
import { EmptyPanel } from "../components/EmptyPanel";
import { ErrorAlert } from "../components/ErrorAlert";
import { PaneTitle } from "../components/PaneTitle";
import { StatusAnnouncer } from "../components/StatusAnnouncer";
import { useFocusHandoff } from "../components/useFocusHandoff";
import { useFadeRegion, useScrollFade } from "../components/scrollFade";
import { isolate } from "../i18n/isolate";
import { useCurrentLanguage, useT } from "../i18n/react";
import { usePlatform } from "../platform/react";
import type { HistoryMessage } from "../protocol";
import { useOctoboardTheme } from "../theme";
import { RendererBoundary } from "../viewer/codeRenderer";
import { CODE_THEMES } from "../viewer/codeTheme";
import { FencedCode } from "../viewer/FencedCode";
import { LINK_COLOURS } from "../viewer/MarkdownSurface";
import { useSessionHistory } from "./useSessionHistory";

// The Markdown renderer, with its library, loads the first time a reply is shown.
const MarkdownDocument = lazy(() => import("../viewer/MarkdownDocument").then((module) => ({ default: module.MarkdownDocument })));

/**
 * What an archived session was about, in place of its terminal: the messages the user sent, told
 * apart from those Octoboard wrote into the session, and on request the agent's final reply to
 * each turn. Read-only; the way back to the terminal is reopening the session.
 */
export function SessionHistoryView({ sessionId }: { sessionId: string }): React.ReactElement {
  const t = useT();
  const { history, reload } = useSessionHistory(sessionId);
  useErrorAcknowledgement(history.state === "failed" ? history.error : undefined, t("history.failed"));
  const [showReplies, setShowReplies] = useState(false);
  const switchLabelId = useId();
  const rootRef = useRef<HTMLElement>(null);
  // The retry button goes away under focus, into the loading state: it moves to the messages when
  // there are any, else to the switch, which is what it finds then.
  const held = useFocusHandoff(() => {
    (rootRef.current?.querySelector<HTMLElement>("[role=region][tabindex]") ?? rootRef.current?.querySelector<HTMLElement>("input"))?.focus();
  });
  const messages = history.state === "loaded" ? history.messages : [];
  const spoken =
    history.state === "loading"
      ? t("history.loading")
      : history.state === "unavailable"
        ? t("history.unavailable.sentence")
        : history.state === "loaded" && messages.length === 0
          ? t("history.empty")
          : undefined;
  return (
    <section ref={rootRef} {...held} aria-label={t("history.title")} className="absolute inset-0 flex flex-col bg-panel">
      {/* A pane header: its height and groove run its bottom line on from the sidebar's and the side
          pane's headers. */}
      <header className="flex h-(--pane-header-height) shrink-0 items-center gap-2 groove-b px-(--pane-inset)">
        <PaneTitle heading={t("history.title")} />
        <div className="control-fills flex shrink-0 items-center gap-2">
          <span id={switchLabelId} className="text-sm">
            {t("history.showReplies")}
          </span>
          <Switch size="sm" aria-labelledby={switchLabelId} isSelected={showReplies} onChange={setShowReplies}>
            <Switch.Content>
              <Switch.Control>
                <Switch.Thumb />
              </Switch.Control>
            </Switch.Content>
          </Switch>
        </div>
      </header>
      <StatusAnnouncer text={spoken} />
      {history.state === "loading" && (
        <div aria-hidden="true" className="flex flex-1 flex-col items-center justify-center-safe gap-3 text-sm text-app-muted">
          <Spinner size="md" color="current" aria-hidden="true" />
          {t("history.loading")}
        </div>
      )}
      {history.state === "failed" && (
        <div className="flex flex-1 flex-col items-center justify-center-safe gap-3 p-4 text-sm">
          <CodeToneContext value="danger">
            <div className="flex w-full min-w-0 max-w-lg flex-col gap-3">
              <ErrorAlert title={t("history.failed")}>
                <DaemonErrorMessage error={history.error} part="summary" />
              </ErrorAlert>
              <DaemonErrorMessage error={history.error} part="details" />
            </div>
          </CodeToneContext>
          <Button size="sm" variant="secondary" preventFocusOnPress onPress={reload}>
            {t("error.tryAgain")}
          </Button>
        </div>
      )}
      {history.state === "unavailable" && (
        <div className="flex flex-1 items-center justify-center-safe">
          <EmptyPanel icon={TriangleAlert} message={t("history.unavailable")} description={t("history.unavailable.description")} silent />
        </div>
      )}
      {history.state === "loaded" &&
        (messages.length === 0 ? (
          <div className="flex flex-1 items-center justify-center-safe">
            <EmptyPanel icon={MessageSquareOff} message={t("history.empty")} silent />
          </div>
        ) : (
          <MessageList messages={messages} showReplies={showReplies} />
        ))}
    </section>
  );
}

/** The messages in a scrolling region of their own, which takes focus so they can be scrolled from
 * the keyboard. The bottom padding clears the floating bar the pane lays over the view. */
function MessageList({ messages, showReplies }: { messages: HistoryMessage[]; showReplies: boolean }): React.ReactElement {
  const t = useT();
  // Always a region, even when the messages fit: it is the pane's F6 landing and what the keyboard
  // scrolls.
  const region = useFadeRegion({ label: t("history.messages"), always: true });
  const fade = useScrollFade({ region, className: "min-h-0 flex-1 overflow-auto px-(--pane-inset) pt-4 pb-24" });
  return (
    <div className={`flex min-h-0 flex-1 flex-col rounded-xl ${region.focused ? "ring-2 ring-app-focus ring-inset" : ""}`}>
      <div {...fade}>
        <ol className="flex flex-col gap-4">
          {messages.map((message, index) =>
            showReplies || message.speaker !== "agent" ? (
              // The index in the whole list, so a message keeps its identity when replies come and go.
              <li key={index}>
                <Message message={message} />
              </li>
            ) : null,
          )}
        </ol>
      </div>
    </div>
  );
}

function Message({ message }: { message: HistoryMessage }): React.ReactElement {
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
  const colours = CODE_THEMES[theme];
  const plain = <p className="break-words whitespace-pre-wrap">{message.text}</p>;
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
      {message.speaker === "agent" ? (
        <div
          className="rounded-xl p-3 text-sm leading-6"
          style={{ backgroundColor: colours.background, color: colours.foreground, "--document-link": LINK_COLOURS[theme] } as React.CSSProperties}
        >
          <RendererBoundary resetKey={message.text} fallback={plain}>
            <Suspense fallback={plain}>
              <MarkdownDocument embedded text={message.text} path="" theme={theme} renderCode={renderCode} onOpenLink={links && onOpenLink} />
            </Suspense>
          </RendererBoundary>
        </div>
      ) : (
        <p
          dir="auto"
          className={`text-sm leading-6 break-words whitespace-pre-wrap ${message.speaker === "octoboard" ? "border-s-2 border-separator ps-3 text-app-muted" : "rounded-xl bg-default px-3 py-2"}`}
        >
          {message.text}
        </p>
      )}
    </article>
  );
}
