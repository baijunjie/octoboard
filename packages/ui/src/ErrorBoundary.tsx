import React, { useEffect } from "react";

import { errorAlerts } from "./errorAlerts";
import { ErrorAlert } from "./components/ErrorAlert";
import { CodeToneContext } from "./components/codeTone";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { Message, useT } from "./i18n/react";
import { StartupHeading, StartupScreen } from "./StartupScreen";

interface ErrorBoundaryState {
  error?: Error;
}

/**
 * Catches a render-time failure anywhere below it and shows what went wrong.
 *
 * Without it React unmounts the whole tree on an uncaught error and leaves a blank window — no
 * message, no way to tell a crashed UI from a daemon that never answered, and nothing to report. A
 * terminal emulator is the kind of dependency that throws from its own scheduled work, so this is
 * the difference between a bad minute and an unexplainable one.
 *
 * The fallback is `StartupScreen` rather than a bare message, because the screen it replaces is the
 * one that was holding the window's quit handlers: rendering anything else would leave the user
 * with an error they cannot even close the window on.
 */
export class ErrorBoundary extends React.Component<{ children: React.ReactNode }, ErrorBoundaryState> {
  state: ErrorBoundaryState = {};

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  render(): React.ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;
    // Clearing the error remounts the subtree's components, which recovers a failure that was
    // transient; one that is not simply lands back here. The daemon connection and its state
    // live outside the tree and are kept as they are.
    return <ErrorScreen error={error} onRetry={() => this.setState({ error: undefined })} />;
  }
}

/** The error's own text for the diagnostic block: its stack, led by the error's name and message
 * where the stack leaves them out, as WebKit's does. */
function diagnostic(error: Error): string {
  const heading = error.message ? `${error.name}: ${error.message}` : error.name;
  if (!error.stack) return heading;
  return error.stack.startsWith(heading) ? error.stack : `${heading}\n${error.stack}`;
}

/** The fallback, a function component because only one can read the current language. The error's
 * own message is raw text, so the Alert says what to do in the application's words and the message
 * stands in the diagnostic block under it. */
function ErrorScreen({ error, onRetry }: { error: Error; onRetry: () => void }): React.ReactElement {
  const t = useT();
  const text = diagnostic(error);
  useEffect(() => { errorAlerts.report({ message: error.message, title: <Message id="error.crashed" />, content: <DiagnosticBlock>{text}</DiagnosticBlock> }); }, [error, text, t]);
  return (
    <StartupScreen
      message={
        <CodeToneContext value="danger">
          <div className="flex min-w-0 flex-col gap-4">
            <StartupHeading><Message id="error.crashed" /></StartupHeading>
            <ErrorAlert announce={false} title={t("error.crashed.alertTitle")}>{t("error.crashed.alertDescription")}</ErrorAlert>
            {/* The stack is taller than the screen more often than not, so the block scrolls (`tall`). */}
            <div className="max-w-[70ch]">
              <DiagnosticBlock tall>{text}</DiagnosticBlock>
            </div>
          </div>
        </CodeToneContext>
      }
      action={{ label: t("error.tryAgain"), onSelect: onRetry }}
    />
  );
}
