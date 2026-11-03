// Renders a component under test inside `TestHost`, in a container that goes with the test.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { onTestFinished } from "vitest";

import type { PlatformAdapter } from "../platform";
import type { Daemon } from "../store";
import { TestHost } from "./TestHost";
import { testDaemon } from "./testDaemon";

/** Renders `children` into a container of its own inside `TestHost` over `daemon` (one that rejects its requests by
 * default) and `platform`, and returns the container, whose `render(ui)` renders `ui` into the same root under the same
 * host. The root is unmounted when the test ends, which runs the effect cleanups and takes what the tree portalled to
 * `<body>` with it. */
export function mountHost(children: React.ReactNode, { daemon = testDaemon(), platform }: { daemon?: Daemon; platform?: PlatformAdapter } = {}): HTMLElement & { render: (children: React.ReactNode) => void } {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const render = (ui: React.ReactNode) => act(() => root.render(<TestHost daemon={daemon} platform={platform}>{ui}</TestHost>));
  render(children);
  return Object.assign(container, { render });
}
