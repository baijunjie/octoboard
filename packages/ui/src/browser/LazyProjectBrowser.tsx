import { Button } from "@heroui/react";
import { TriangleAlert } from "lucide-react";
import React, { lazy, Suspense, useEffect, useRef, useState } from "react";

import { useT } from "../i18n/react";
import { AsidePane, type AsideLayout } from "../layout/AsidePane";
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
 * what it brings is needed only then. While it loads the aside keeps its place, empty; a load (or a
 * render) that fails shows in the aside, with a way to try again, rather than taking the window with
 * it. The failure takes keyboard focus when what held it went down with the browser, and gives it
 * to the terminal when Try again replaces it by the empty pane of another load.
 */
export function LazyProjectBrowser({ focusTerminal, ...props }: Props): React.ReactElement {
  const [, retried] = useState(0);
  return (
    <LoadBoundary
      layout={props.layout}
      focusTerminal={focusTerminal}
      onRetry={() => {
        Browser = load();
        retried((count) => count + 1);
      }}
    >
      <Suspense fallback={<AsidePane layout={props.layout} />}>
        <Browser {...props} />
      </Suspense>
    </LoadBoundary>
  );
}

class LoadBoundary extends React.Component<
  { layout: AsideLayout; onRetry: () => void; focusTerminal: () => void; children: React.ReactNode },
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
        layout={this.props.layout}
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
function LoadFailed({ layout, onRetry }: { layout: AsideLayout; onRetry: (holdsFocus: boolean) => void }): React.ReactElement {
  const t = useT();
  const retry = useRef<HTMLButtonElement>(null);
  // A render that failed with focus inside the browser took focus down with it.
  useEffect(() => {
    if (document.activeElement === document.body) retry.current?.focus();
  }, []);
  return (
    <AsidePane layout={layout} className="control-fills items-center justify-center gap-3 px-4 text-center text-sm">
      <TriangleAlert aria-hidden="true" className="size-6 text-danger" />
      <p role="alert">{t("browser.loadFailed")}</p>
      <Button
        ref={retry}
        size="sm"
        variant="secondary"
        preventFocusOnPress
        onPress={() => onRetry(document.activeElement === retry.current)}
      >
        {t("error.tryAgain")}
      </Button>
    </AsidePane>
  );
}
