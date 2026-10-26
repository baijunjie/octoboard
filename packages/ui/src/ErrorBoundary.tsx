import React, { useEffect } from "react";

import { errorAlerts } from "./errorAlerts";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { Message, useT } from "./i18n/react";
import { StartupScreen } from "./StartupScreen";

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
 * own text is raw, so the page says what happened and what to do in the application's words and the
 * text is in the popover of the mark after the heading. */
function ErrorScreen({ error, onRetry }: { error: Error; onRetry: () => void }): React.ReactElement {
  const t = useT();
  const text = diagnostic(error);
  // Reported once per error: the alert's words are components that follow the language, so a change of language needs
  // no new report. The alert says it in the application's words; the error's own text is its "Error details".
  useEffect(() => {
    errorAlerts.report({
      message: error.message,
      title: <Message id="error.crashed.heading" />,
      content: <p><Message id="error.crashed" /></p>,
      details: <DiagnosticBlock wrap tall>{text}</DiagnosticBlock>,
    });
  }, [error, text]);
  return (
    <StartupScreen
      failure={{
        title: t("error.crashed.heading"),
        description: t("error.crashed.retry"),
        markLabel: t("error.crashed.markLabel"),
        // The stack is taller than the popover more often than not, so the block scrolls (`tall`), and its lines are
        // wider, so they wrap rather than run past its edge.
        details: <DiagnosticBlock wrap tall>{text}</DiagnosticBlock>,
      }}
      action={{ label: t("error.tryAgain"), onSelect: onRetry }}
    />
  );
}
