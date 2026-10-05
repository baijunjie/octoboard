import React from "react";

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
    return (
      <StartupScreen
        message={
          <>
            Octoboard hit an error it could not recover from.
            <pre className="mt-4 max-w-full overflow-auto text-left text-xs">{error.stack ?? error.message}</pre>
          </>
        }
        // Clearing the error remounts the subtree's components, which recovers a failure that was
        // transient; one that is not simply lands back here. The daemon connection and its state
        // live outside the tree and are kept as they are.
        action={{ label: "Try again", onSelect: () => this.setState({ error: undefined }) }}
      />
    );
  }
}
