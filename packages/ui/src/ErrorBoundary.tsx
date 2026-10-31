import React from "react";

import { errorDiagnostic } from "./errorText";
import { DiagnosticBlock } from "./components/DiagnosticBlock";
import { useT } from "./i18n/react";
import { useAppExit } from "./lifecycle/useAppExit";
import { StartupFailure, StartupScreen } from "./StartupScreen";

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
 * The fallback is `StartupScreen` rather than a bare message: the application had registered with
 * the shell's exit flow, which has no unregister, so the fallback registers again (`ErrorScreen`),
 * and rendering anything else would leave the user with an error they cannot even close the
 * window on.
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

/** The fallback, a function component because only one can read the current language. The error's
 * own text is raw, so the page says what happened and what to do in the application's words and the
 * text sits on a surface of its own under them, and is also what the page's report is made from. */
function ErrorScreen({ error, onRetry }: { error: Error; onRetry: () => void }): React.ReactElement {
  const t = useT();
  const text = errorDiagnostic(error);
  // The application registered with the shell before it crashed, and the shell has no unregister, so it would go on
  // deferring quits and window closes to a page that no longer answers. Registering again answers them by quitting at
  // once, with no button: it deliberately asks about no sessions and shuts no daemon down, since the application that
  // knew them is gone, and the daemon is left to end with the shell.
  useAppExit();
  return (
    <StartupScreen
      message={
        <StartupFailure
          failure={{
            title: t("error.crashed.heading"),
            description: t("error.crashed"),
            remedy: t("error.crashed.retry"),
            details: <DiagnosticBlock>{text}</DiagnosticBlock>,
          }}
          action={{ label: t("error.tryAgain"), onSelect: onRetry }}
          diagnostic={text}
        />
      }
    />
  );
}
