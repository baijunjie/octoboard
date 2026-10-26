import React, { useEffect } from "react";

import { Button } from "./components/Button";
import { errorAlerts } from "./errorAlerts";
import { ContentPanel } from "./components/ContentPanel";
import { ErrorAlert } from "./components/ErrorAlert";
import { CodeToneContext } from "./components/codeTone";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { InlineReference } from "./components/InlineReference";
import { BareTitleBar } from "./components/TitleBar";
import { FadedScroller, useFadeRegion } from "./components/scrollFade";
import { Message, useT } from "./i18n/react";
import { useAppExit } from "./lifecycle/useAppExit";

interface StartupScreenProps {
  message: React.ReactNode;
  /** An extra button beside Quit, for a screen that has something to offer besides giving up. */
  action?: { label: string; onSelect: () => void };
}

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
export function StartupScreen({ message, action }: StartupScreenProps): React.ReactElement {
  const t = useT();
  const { requestQuit, canQuit } = useAppExit();
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
              <div className="flex gap-2">
                {action && <Button onPress={action.onSelect}>{action.label}</Button>}
                {canQuit && <Button onPress={() => void requestQuit()}>{t("common.quit")}</Button>}
              </div>
            </div>
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

/** What the screen says when the shell passed the daemon's startup failure along as `?error=`. */
export function DaemonFailedMessage({ error }: { error: string }): React.ReactElement {
  const t = useT();
  useEffect(() => {
    // The alert says it in the application's words; the raw text is its "Error details", on a surface of its own.
    errorAlerts.report({
      message: error,
      title: t("startup.daemonFailed.title"),
      content: <p>{t("startup.daemonFailed.description")}</p>,
      details: <DiagnosticBlock wrap>{error}</DiagnosticBlock>,
    });
  }, [error, t]);
  return (
    <CodeToneContext value="danger">
      <div className="flex min-w-0 flex-col gap-4">
        <StartupHeading>{t("startup.daemonFailed.title")}</StartupHeading>
        <ErrorAlert announce={false} title={t("startup.daemonFailed.alertTitle")}>{t("startup.daemonFailed.description")}</ErrorAlert>
        <DiagnosticBlock wrap>{error}</DiagnosticBlock>
      </div>
    </CodeToneContext>
  );
}

/** What the screen says when there is no daemon address to connect to, and how to give it one. */
export function NoAddressMessage(): React.ReactElement {
  const t = useT();
  const code = (text: string) => <InlineReference monospace wrap>{text}</InlineReference>;
  return (
    <CodeToneContext value="danger">
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
    </CodeToneContext>
  );
}
