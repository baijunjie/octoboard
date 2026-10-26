import React, { lazy, Suspense, useState } from "react";

import { ActionableError } from "../errorRecovery";
import { useT } from "../i18n/react";
import { useDaemon } from "../store";
import type { TerminalFileViewer as TerminalFileViewerType } from "./TerminalFileViewer";

const load = () => lazy(() => import("./TerminalFileViewer").then((module) => ({ default: module.TerminalFileViewer })));
/** The viewer, loaded the first time one is shown, and loaded again after a failed load: `lazy`
 * keeps a failure, so the next click needs a new one. */
let Viewer = load();

/**
 * The file viewer a path in the terminal asked for, loaded the first time it is shown, with the code
 * renderer it brings. Nothing shows while it loads. A load (or a render) that fails closes it and
 * says so in an error alert, and the next click tries the load again.
 */
export function LazyTerminalFileViewer(props: React.ComponentProps<typeof TerminalFileViewerType>): React.ReactElement {
  // Kept out of the React Compiler: a failed load puts a new `Viewer` in place of the module's, which
  // its memoization would take for constant and so keep rendering the one that failed.
  "use no memo";
  const t = useT();
  const { toastError } = useDaemon();
  const [, retried] = useState(0);
  return (
    <LoadBoundary
      onFailed={() => {
        Viewer = load();
        retried((count) => count + 1);
        // A known failure: the body says to open the file again, and it is not reported as a bug.
        toastError(new ActionableError(t("viewer.loadFailed"), "none"), undefined, t("viewer.loadFailedTitle"));
        props.onClose();
      }}
    >
      <Suspense fallback={null}>
        <Viewer {...props} />
      </Suspense>
    </LoadBoundary>
  );
}

class LoadBoundary extends React.Component<{ onFailed: () => void; children: React.ReactNode }, { failed: boolean }> {
  state = { failed: false };

  static getDerivedStateFromError(): { failed: boolean } {
    return { failed: true };
  }

  componentDidCatch(error: unknown): void {
    console.warn("Viewer: the file viewer failed to load", error);
    this.props.onFailed();
  }

  render(): React.ReactNode {
    return this.state.failed ? null : this.props.children;
  }
}
