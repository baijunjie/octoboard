import { CircleAlert } from "lucide-react";
import React from "react";

import { ContentPanel } from "./components/ContentPanel";
import { EmptyPanel } from "./components/EmptyPanel";
import { ErrorAlert } from "./components/ErrorAlert";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { InlineReference } from "./components/InlineReference";
import { BareTitleBar } from "./components/TitleBar";
import { FadedScroller, useFadeRegion } from "./components/scrollFade";
import { Message, useT } from "./i18n/react";
import { useIssueReport } from "./useIssueReport";

/**
 * The window with no daemon connection behind it: no address known yet, the daemon failed to
 * start, or the UI itself crashed. It has no Quit button and registers nothing with the shell's
 * exit flow, so the shell quits on Cmd+Q, the app menu and the window's close button natively.
 * `message` is what the screen says: `NoAddressMessage`, or a `StartupFailure`.
 */
export function StartupScreen({ message }: { message: React.ReactNode }): React.ReactElement {
  const t = useT();
  // Reachable from the keyboard while it scrolls, as a dialog's body is: the buttons sit at the end
  // of the content, so a long message has nothing between them and the top to take focus, and the
  // window itself never scrolls.
  const region = useFadeRegion({ label: t("startup.message") });

  return (
    <div className="flex h-full flex-col">
      <BareTitleBar />
      <ContentPanel bare>
        {/* The window never scrolls (`style.css`), so content taller than it scrolls here, under the
            title bar; `m-auto` centres it while it fits, where `justify-center` would clip its top.
            The ring is drawn on the wrapper around the scroller, inset because the scroller spans the
            window. */}
        <main className={`flex min-h-0 flex-1 flex-col ${region.focused ? "ring-2 ring-focus ring-inset" : ""}`}>
          <FadedScroller region={region} className="flex min-h-0 flex-1 flex-col">
            <div className="m-auto flex w-full min-w-0 flex-col items-center gap-4 p-8">
              <div className="w-full min-w-0 max-w-xl">{message}</div>
            </div>
          </FadedScroller>
        </main>
      </ContentPanel>
    </div>
  );
}

/**
 * A failure that is the whole page: the empty state's shape in the danger tone, its heading `title`, the line under it
 * `description` in the application's own words, the `remedy` after that, and `details` (any further explanation, then
 * the raw text) on a surface of its own under them (see "A whole subject that failed" in `docs/ui-accessibility.md`). It
 * is the failure's only presentation, so the page announces its heading and `description`, inserted after the first
 * paint. `action` is the failure's recovery, its primary button.
 *
 * Report issue stands after the recovery, or alone, built from `diagnostic`, the plain text of the failure. By default
 * the failure is a possible bug (a crash): the remedy line already asks for the report, so the sentence about it only
 * stands where there is no link to press. A `known` failure is one the application names and says what to do about
 * (`title` is the Issue's English title): it is not called a bug, and its sentence stays above the raw text.
 */
export function StartupFailure({ failure, action, diagnostic, known }: {
  failure: { title: React.ReactNode; description: React.ReactNode; remedy?: React.ReactNode; details: React.ReactNode };
  action?: { label: string; onSelect: () => void };
  diagnostic: string;
  known?: { title: string };
}): React.ReactElement {
  const issue = useIssueReport([{ error: diagnostic, unexpected: true }], { size: "sm", known });
  return (
    <EmptyPanel
      tone="danger"
      icon={CircleAlert}
      headingLevel={1}
      message={failure.title}
      description={failure.description}
      remedy={failure.remedy}
      announce
      // After the first paint, so the alert is a change to the page rather than part of its first render.
      announceDelayMs={0}
      action={action ? { label: action.label, variant: "primary", onPress: action.onSelect } : undefined}
      linkAction={issue.link}
      details={<>{(known || !issue.link) && <p>{issue.guidance}</p>}{failure.details}</>}
    />
  );
}

/** The heading of a startup screen's message, centred over the rest of it, which starts at the edge. */
export function StartupHeading({ children }: { children: React.ReactNode }): React.ReactElement {
  return <h1 className="text-center text-xl font-semibold text-foreground">{children}</h1>;
}

/** The screen for a daemon that failed to start, which the shell passed along as `?error=`: the failure in the
 * application's own words, with the daemon's own message under it, which is also what its report is built from. */
export function DaemonFailedScreen({ error }: { error: string }): React.ReactElement {
  const t = useT();
  return (
    <StartupScreen
      message={
        <StartupFailure
          failure={{
            title: t("startup.daemonFailed.title"),
            description: t("startup.daemonFailed.description"),
            remedy: t("startup.daemonFailed.remedy"),
            details: <DiagnosticBlock>{error}</DiagnosticBlock>,
          }}
          diagnostic={error}
          known={{ title: "The daemon failed to start" }}
        />
      }
    />
  );
}

/** What the screen says when there is no daemon address or key to connect with, and how to give them. */
export function NoAddressMessage(): React.ReactElement {
  const t = useT();
  const code = (text: string) => <InlineReference monospace wrap unbroken>{text}</InlineReference>;
  // The report describes the failure in fixed English rather than the screen's localized wording, which a report never
  // carries.
  const issue = useIssueReport(
    [{ error: "The UI was given no daemon address or key to connect with.", unexpected: true }],
    { size: "sm", known: { title: "No daemon address or key" } },
  );
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <StartupHeading>{t("startup.noAddress.title")}</StartupHeading>
      <ErrorAlert announce={false} title={t("startup.noAddress.alertTitle")}>{t("startup.noAddress.description")}</ErrorAlert>
      <p>
        <Message id="startup.noAddress.shell" params={{ port: code("?port="), key: code("?key=") }} />
      </p>
      <p>
        <Message id="startup.noAddress.dev" params={{ dev: code("vite dev") }} />
      </p>
      <ul className="list-disc space-y-3 ps-5">
        <li>
          <Message id="startup.noAddress.query" params={{ port: code("?port="), key: code("?key=") }} />
        </li>
        <li>
          <Message id="startup.noAddress.env" params={{ portVar: code("VITE_DAEMON_PORT"), keyVar: code("VITE_DAEMON_KEY") }} />
        </li>
        <li>
          <Message id="startup.noAddress.proxy" params={{ proxyVar: code("OCTOBOARD_DAEMON_PORT"), key: code("?key="), keyVar: code("VITE_DAEMON_KEY") }} />
        </li>
      </ul>
      <p>{issue.guidance}</p>
      {issue.link && <div className="flex justify-center">{issue.link}</div>}
    </div>
  );
}
