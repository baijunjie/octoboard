// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { PlatformAdapter, StatusItemCapability } from "../platform";
import { PlatformProvider } from "../platform/react";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { useStatusItemMenu } from "./useStatusItemMenu";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function Host() {
  useStatusItemMenu({ ready: true, onSessionChosen: () => {} });
  return null;
}

function mount(statusItem: StatusItemCapability, request: Daemon["request"]) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const daemon = { store: createStateStore({}), request } as unknown as Daemon;
  act(() =>
    root.render(
      <PlatformProvider value={{ statusItem } as PlatformAdapter}>
        <DaemonProvider value={daemon}>
          <Host />
        </DaemonProvider>
      </PlatformProvider>,
    ),
  );
}

it("asks the daemon to refresh the quotas that are due each time the menu begins to open", () => {
  let opened: () => void = () => {};
  const undo = vi.fn();
  const statusItem: StatusItemCapability = {
    setMenu: () => Promise.resolve(),
    onSessionChosen: () => () => {},
    onOpened(handler) {
      opened = handler;
      return undo;
    },
  };
  const request = vi.fn(() => Promise.reject(new Error("offline")));
  mount(statusItem, request as unknown as Daemon["request"]);
  expect(request).not.toHaveBeenCalled();
  opened();
  opened();
  expect(request.mock.calls).toEqual([[{ type: "refresh_account_quotas" }], [{ type: "refresh_account_quotas" }]]);
});
