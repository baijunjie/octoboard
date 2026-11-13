import React, { Suspense } from "react";

import { trackSuspenseFallback } from "../../components/hooks/lazyReadiness";
import { ActionableError } from "../../errors/actionableError";
import { useT } from "../../i18n/react";
import { useServedByDaemon } from "../useServedByDaemon";
import { retryableLazy } from "../../components/hooks/retryableLazy";
import { raiseError } from "../../components/alerts/raiseError";

/** The viewer, loaded the first time one is shown, and loaded again after a failed load. */
const viewer = retryableLazy(() => import("./LinkedFileViewer").then((module) => ({ default: module.LinkedFileViewer })));
const Viewer = viewer.Component;

/**
 * The file viewer a path in the terminal asked for, loaded the first time it is shown, with the code
 * renderer it brings. Nothing shows while it loads. A load (or a render) that fails closes it and
 * says so in an error alert, and the next click tries the load again.
 */
export function LazyLinkedFileViewer(props: React.ComponentProps<typeof Viewer>): React.ReactElement {
  const t = useT();
  const servedByDaemon = useServedByDaemon();
  return (
    <LoadBoundary
      onFailed={() => {
        viewer.retry();
        // A known failure: the body says what to do, and it is not reported as a bug. In a page the daemon serves, that
        // is to reload it, since the daemon no longer has the file the page asks for.
        raiseError(new ActionableError(t(servedByDaemon ? "viewer.loadFailed.reload" : "viewer.loadFailed"), "none"), { title: t("viewer.loadFailedTitle") });
        props.onClose();
      }}
    >
      <Suspense fallback={trackSuspenseFallback(null)}>
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
