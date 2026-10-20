import React, { lazy, Suspense, useEffect, useRef, useState } from "react";

import { Button } from "../components/Button";
import { ErrorAlert } from "../components/ErrorAlert";
import { useErrorAcknowledgement } from "../components/useErrorAcknowledgement";
import { useT } from "../i18n/react";
import { PanelHeader } from "../components/PanelHeader";
import type { ProjectBrowser as ProjectBrowserType } from "./ProjectBrowser";

type Props = React.ComponentProps<typeof ProjectBrowserType> & {
  /** Where keyboard focus goes when what held it in the pane goes away. */
  focusTerminal: () => void;
};

const load = () => lazy(() => import("./ProjectBrowser").then((module) => ({ default: module.ProjectBrowser })));
/** The component, loaded the first time a project's browser is shown, and loaded again after a
 * failed load is retried: `lazy` keeps a failure, so a retry needs a new one. */
let Browser = load();

/**
 * A project's browser, with the file viewer it opens, loaded the first time one is shown: most of
 * what it brings is needed only then. While it loads its panel keeps its place, empty; a load (or a
 * render) that fails shows in the panel, with a way to try again, rather than taking the window with
 * it. The failure takes keyboard focus when what held it went down with the browser, and gives it
 * to the terminal when Try again replaces it by the empty pane of another load.
 */
export function LazyProjectBrowser({ focusTerminal, ...props }: Props): React.ReactElement {
  // Kept out of the React Compiler: a retry puts a new `Browser` in place of the module's, which its
  // memoization would take for constant and so keep rendering the one that failed.
  "use no memo";
  const [, retried] = useState(0);
  return (
    <LoadBoundary
      title={props.project.name}
      focusTerminal={focusTerminal}
      onRetry={() => {
        Browser = load();
        retried((count) => count + 1);
      }}
    >
      <Suspense fallback={null}>
        <Browser {...props} />
      </Suspense>
    </LoadBoundary>
  );
}

class LoadBoundary extends React.Component<
  { title: string; onRetry: () => void; focusTerminal: () => void; children: React.ReactNode },
  { failed: boolean }
> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  render(): React.ReactNode {
    if (!this.state.failed) return this.props.children;
    return (
      <LoadFailed
        title={this.props.title}
        onRetry={(holdsFocus) => {
          if (holdsFocus) this.props.focusTerminal();
          this.props.onRetry();
          this.setState({ failed: false });
        }}
      />
    );
  }
}

/** The fallback, a function component because only one can read the current language. */
function LoadFailed({ title, onRetry }: { title: string; onRetry: (holdsFocus: boolean) => void }): React.ReactElement {
  const t = useT();
  useErrorAcknowledgement(t("browser.loadFailed"));
  const retry = useRef<HTMLButtonElement>(null);
  // A render that failed with focus inside the browser took focus down with it.
  useEffect(() => {
    if (document.activeElement === document.body) retry.current?.focus();
  }, []);
  return (
    <>
      <PanelHeader title={title} />
      <div className="flex min-h-0 min-w-0 flex-1 flex-col items-center justify-center gap-3 p-4 text-sm">
        <ErrorAlert className="w-full max-w-lg">{t("browser.loadFailed")}</ErrorAlert>
        <Button
          ref={retry}
          data-region-landing=""
          size="sm"
          variant="secondary"
          preventFocusOnPress
          onPress={() => onRetry(document.activeElement === retry.current)}
        >
          {t("error.tryAgain")}
        </Button>
      </div>
    </>
  );
}
