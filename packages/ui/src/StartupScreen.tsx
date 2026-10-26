import { CircleAlert } from "lucide-react";
import React, { useEffect } from "react";

import { Button } from "./components/Button";
import { errorAlerts } from "./errorAlerts";
import { ContentPanel } from "./components/ContentPanel";
import { EmptyPanel } from "./components/EmptyPanel";
import { ErrorAlert } from "./components/ErrorAlert";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { InlineReference } from "./components/InlineReference";
import { StatusMark } from "./components/StatusMark";
import { BareTitleBar } from "./components/TitleBar";
import { FadedScroller, useFadeRegion } from "./components/scrollFade";
import { Message, useT } from "./i18n/react";
import { useAppExit } from "./lifecycle/useAppExit";

type StartupScreenProps =
  | {
    /** What the screen explains, over its Quit. */
    message: React.ReactNode;
    failure?: never;
    action?: never;
  }
  | {
    /** A failure that is the whole page: the empty state's shape in the danger tone, its heading `title`, the line
     * under it `description` in the application's own words, and `details` (the raw text) in the popover of a mark
     * right after the heading, never on the page (see "A whole subject that failed" in
     * `docs/ui-accessibility.md`). */
    failure: { title: React.ReactNode; description: React.ReactNode; markLabel: string; details: React.ReactNode };
    message?: never;
    /** The failure's recovery, its primary button, for a screen that has something to offer besides giving up. */
    action?: { label: string; onSelect: () => void };
  };

/**
 * The window with no daemon connection behind it: no address known yet, the daemon failed to
 * start, or the UI itself crashed. Where the platform has a quit flow it always carries its own
 * quit affordance, and that is not cosmetic — the Rust side only lets a window block a quit once
 * some screen has mounted `useAppExit`, and a screen that registers without offering a way out
 * would leave the window unquittable. A plain browser has no quit flow, so there is no Quit.
 *
 * `getSessions`/`requestShutdown` are both omitted: there is no connection here to ask about
 * sessions or to shut down, so quitting happens immediately with no confirmation.
 */
export function StartupScreen({ message, failure, action }: StartupScreenProps): React.ReactElement {
  const t = useT();
  const { requestQuit, canQuit } = useAppExit();
  // Reachable from the keyboard while it scrolls, as a dialog's body is: the buttons sit at the end
  // of the content, so a long message has nothing between them and the top to take focus, and the
  // window itself never scrolls.
  const region = useFadeRegion({ label: t("startup.message") });
  const quit = canQuit ? { label: t("common.quit"), onPress: () => void requestQuit() } : undefined;

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
            {failure ? (
              <div className="m-auto w-full min-w-0 max-w-xl p-8">
                {/* Quit stands beside the recovery as the second way out; with no recovery it is the one action. The
                    raw text is the mark's popover, so the row never holds more than the two buttons. */}
                <EmptyPanel
                  tone="danger"
                  icon={CircleAlert}
                  headingLevel={1}
                  message={failure.title}
                  description={failure.description}
                  action={action ? { label: action.label, variant: "primary", onPress: action.onSelect } : quit}
                  secondaryAction={action ? quit : undefined}
                  headingMark={<StatusMark label={failure.markLabel}>{failure.details}</StatusMark>}
                />
              </div>
            ) : (
              <div className="m-auto flex w-full min-w-0 flex-col items-center gap-4 p-8">
                <div className="w-full min-w-0 max-w-xl">{message}</div>
                {quit && <Button onPress={quit.onPress}>{quit.label}</Button>}
              </div>
            )}
          </FadedScroller>
        </main>
      </ContentPanel>
    </div>
  );
}

/** The heading of a startup screen's message, centred over the rest of it, which starts at the edge. */
export function StartupHeading({ children }: { children: React.ReactNode }): React.ReactElement {
  return <h1 className="text-center text-xl font-semibold text-foreground">{children}</h1>;
}

/** The screen for a daemon that failed to start, which the shell passed along as `?error=`: the failure in the
 * application's own words, with the daemon's own message in the popover of the mark after the heading. */
export function DaemonFailedScreen({ error }: { error: string }): React.ReactElement {
  const t = useT();
  // Reported once per error: the alert's words are components that follow the language, so a change of language needs no new report.
  useEffect(() => {
    // The alert says it in the application's words; the raw text is its "Error details", on a surface of its own.
    errorAlerts.report({
      message: error,
      title: <Message id="startup.daemonFailed.title" />,
      content: <p><Message id="startup.daemonFailed.description" /></p>,
      details: <DiagnosticBlock wrap>{error}</DiagnosticBlock>,
    });
  }, [error]);
  return (
    <StartupScreen
      failure={{
        title: t("startup.daemonFailed.title"),
        description: t("startup.daemonFailed.remedy"),
        markLabel: t("startup.daemonFailed.markLabel"),
        // A popover's height is bounded, so a long message scrolls within it (`tall`).
        details: <DiagnosticBlock wrap tall>{error}</DiagnosticBlock>,
      }}
    />
  );
}

/** What the screen says when there is no daemon address to connect to, and how to give it one. */
export function NoAddressMessage(): React.ReactElement {
  const t = useT();
  const code = (text: string) => <InlineReference monospace wrap>{text}</InlineReference>;
  return (
    <div className="flex min-w-0 flex-col gap-4">
      <StartupHeading>{t("startup.noAddress.title")}</StartupHeading>
      <ErrorAlert announce={false} title={t("startup.noAddress.alertTitle")}>{t("startup.noAddress.description")}</ErrorAlert>
      <p>
        <Message id="startup.noAddress.shell" params={{ port: code("?port=") }} />
      </p>
      <p>
        <Message id="startup.noAddress.dev" params={{ dev: code("vite dev") }} />
      </p>
      <ul className="list-disc space-y-3 ps-5">
        <li>
          <Message id="startup.noAddress.query" params={{ port: code("?port=") }} />
        </li>
        <li>
          <Message id="startup.noAddress.env" params={{ portVar: code("VITE_DAEMON_PORT") }} />
        </li>
        <li>
          <Message id="startup.noAddress.proxy" params={{ proxyVar: code("OCTOBOARD_DAEMON_PORT") }} />
        </li>
      </ul>
    </div>
  );
}
