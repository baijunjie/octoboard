import React from "react";

import { ErrorAlert } from "./components/ErrorAlert";
import { CodeBlock } from "./components/InlineCode";
import { FadedScroller, useFadeRegion } from "./components/scrollFade";
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
  // Reachable from the keyboard while it scrolls, as a dialog's body is: nothing else on the screen
  // would scroll the stack.
  const region = useFadeRegion({ label: t("error.stack") });
  return (
    <StartupScreen
      message={
        <ErrorAlert title={t("error.crashed")} headingLevel={1}>
          {/* The stack is taller than the screen more often than not, so it fades at the edge it
              hides the rest behind. The ring goes on this wrapper, which draws it outside its own box, so
              it needs no room of its own. `overflow-auto` beside the fade's own `overflow-y`: a long
              frame scrolls sideways too, and only the vertical edge fades. */}
          <div className={`mt-4 max-w-[70ch] rounded-lg ${region.focused ? "ring-2 ring-app-focus" : ""}`}>
            <FadedScroller region={region} dir="ltr" className="max-h-[40vh] overflow-auto rounded-lg">
              {/* A stack trace is program text: its indentation and its frames read left to right in
                  any interface language. */}
              <CodeBlock dir="ltr" wrap={false} className="text-xs">
                {error.stack ?? error.message}
              </CodeBlock>
            </FadedScroller>
          </div>
        </ErrorAlert>
      }
      action={{ label: t("error.tryAgain"), onSelect: onRetry }}
    />
  );
}
