// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";

import type { Session } from "../protocol";
import { consoleOf, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { SessionDialog } from "./SessionDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const parentConsole = consoleOf("c-1", "Main");
const project = projectOf("p-1", parentConsole.id, "Search API");

function fakeDaemon(): Daemon {
  return {
    store: createStateStore(),
    request: vi.fn(),
    toastError: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

/** Renders the dialog and returns its "report to console session" checkbox input. */
function renderCheckbox(sessions: Session[]): HTMLInputElement {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={fakeDaemon()}>
        <SessionDialog console={parentConsole} project={project} sessions={sessions} onClose={() => {}} onOpened={() => {}} />
      </DaemonProvider>,
    ),
  );
  const checkbox = document.body.querySelector('input[type="checkbox"]') as HTMLInputElement;
  act(() => root.unmount());
  container.remove();
  return checkbox;
}

it("disables the report-to-console-session checkbox when the console has no live console session", () => {
  const checkbox = renderCheckbox([]);
  expect(checkbox.disabled).toBe(true);
});

it("enables the checkbox once a live console session exists", () => {
  const checkbox = renderCheckbox([sessionOf("s-console", parentConsole.id, undefined, "Hub 1", "idle")]);
  expect(checkbox.disabled).toBe(false);
});
