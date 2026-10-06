import React from "react";

import { useT } from "./i18n/react";
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

/** The fallback, a function component because only one can read the current language. */
function ErrorScreen({ error, onRetry }: { error: Error; onRetry: () => void }): React.ReactElement {
  const t = useT();
  return (
    <StartupScreen
      message={
        <>
          {t("error.crashed")}
          <pre className="mt-4 max-h-[40vh] max-w-[70ch] overflow-auto rounded-lg bg-surface p-3 text-left text-xs">{error.stack ?? error.message}</pre>
        </>
      }
      action={{ label: t("error.tryAgain"), onSelect: onRetry }}
    />
  );
}
