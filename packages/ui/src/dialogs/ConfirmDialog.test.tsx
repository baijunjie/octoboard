// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { ConfirmDialog } from "./ConfirmDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function fakeDaemon(): Daemon {
  return {
    store: createStateStore({}),
    request: vi.fn(),
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

/** Renders a confirmation and takes it down when the test finishes. */
function mount(props: { destructive?: boolean; typeToConfirm?: string }): void {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  act(() => {
    root.render(
      <DaemonProvider value={fakeDaemon()}>
        <ConfirmDialog title="Archive" message="Archive it?" confirmLabel="Archive" {...props}
          onConfirm={async () => {}} onCancel={() => {}} />
      </DaemonProvider>,
    );
  });
}

// Enter confirms a plain confirmation as soon as it opens, which needs focus on its confirm button;
// a destructive one leaves focus on the dialog so a stray Enter does nothing, and one asking for a
// typed word starts in its field. (The unprompted questions are covered where they are asked, in
// `TrustPromptDialog.test.tsx`.)
it.each([
  ["a plain confirmation", {}, "button[type=submit]"],
  ["a destructive confirmation", { destructive: true }, "[role=alertdialog]"],
  ["a typed confirmation", { typeToConfirm: "delete" }, "input"],
])("starts focus in the right place for %s", (_name, props, focused) => {
  mount(props);
  expect(document.activeElement?.matches(focused)).toBe(true);
});
