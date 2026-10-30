// Shared by the `SessionDialog` test files, which split the dialog's tests by area.
import { act } from "react";
import { createRoot } from "react-dom/client";
import { vi } from "vitest";

import type { Session } from "../protocol";
import type { NewSessionBinding } from "./dialogRequest";
import { type ConsoleExtra, consoleOf, type ProjectExtra, projectOf, sessionOf } from "../gallery/fixtures/builders";
import { createStateStore, DaemonProvider, type Daemon, type State } from "../store";
import { trackMountRefocus } from "./dialogTestSupport";
import { SessionDialog } from "./SessionDialog";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

export const parentConsole = consoleOf("c-1", "Main");
export const project = projectOf("p-1", parentConsole.id, "Search API");

function fakeDaemon(initial: Partial<State> = {}): Daemon {
  return {
    store: createStateStore(initial),
    request: vi.fn(async () => ({ type: "ack", id: null })) as Daemon["request"],
    toastError: vi.fn(),
    toastNotice: vi.fn(),
    onToast: () => () => {},
    dismissTrustPrompt: () => {},
    reconnect: () => {},
    terminalUrl: () => "",
  };
}

/** Renders the dialog and returns its container, root and daemon, so a test can query the rendered
 * body, read the requests sent and unmount it, along with `pastMountRefocus`, which the test awaits
 * before a step whose focus it asserts (see `trackMountRefocus`). */
export function renderDialog(
  sessions: Session[],
  initial: Partial<State> = {},
  consoleOverride: ConsoleExtra = {},
  binding: NewSessionBinding = { kind: "choose" },
  projectOverride: ProjectExtra | null = {},
): { container: HTMLElement; root: ReturnType<typeof createRoot>; daemon: Daemon; onClose: () => void; onOpened: (sessionId: string) => void; onResume: ReturnType<typeof vi.fn<(sessionId: string) => Promise<void>>>; pastMountRefocus: () => Promise<void> } {
  const daemon = fakeDaemon(initial);
  const onClose = vi.fn();
  const onOpened = vi.fn();
  const onResume = vi.fn<(sessionId: string) => Promise<void>>(async () => {});
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <SessionDialog
          console={consoleOf(parentConsole.id, parentConsole.name, consoleOverride)}
          project={projectOverride === null ? undefined : projectOf(project.id, parentConsole.id, project.name, projectOverride)}
          binding={binding}
          sessions={sessions}
          onClose={onClose}
          onOpened={onOpened}
          onResume={onResume}
        />
      </DaemonProvider>,
    ),
  );
  return { container, root, daemon, onClose, onOpened, onResume, pastMountRefocus: trackMountRefocus() };
}

export const hub1 = sessionOf("s-hub-1", parentConsole.id, undefined, "Hub", "idle", { colour: "teal", started_at: 100 });

export const buttons = () => Array.from(document.body.querySelectorAll("button"));
export const optionElements = () => Array.from(document.body.querySelectorAll<HTMLElement>('[role="option"]'));

export const choose: NewSessionBinding = { kind: "choose" };

export const pickerNamed = (name: string) => buttons().find((button) =>
  button.getAttribute("aria-label") === name ||
  button.getAttribute("aria-labelledby")?.split(" ").some((id) => document.getElementById(id)?.textContent === name),
)!;
