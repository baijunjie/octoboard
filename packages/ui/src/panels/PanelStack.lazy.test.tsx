// @vitest-environment jsdom
import { FolderClosed } from "lucide-react";
import { act, lazy, Suspense, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { PanelHeader } from "../components/PanelHeader";
import type { Project } from "../protocol";
import { PanelStack } from "./PanelStack";

// A panel whose code arrives after the stack has mounted and settled, as the Files browser's does.
vi.mock("./panelRegistry", () => {
  const Inner = ({ switching }: { switching: boolean }) => {
    const [atMount] = useState(switching);
    return (
      <>
        <PanelHeader title="Files" />
        <button type="button" data-switching={String(atMount)}>inside</button>
      </>
    );
  };
  const Loaded = lazy(() => new Promise<{ default: typeof Inner }>((resolve) => setTimeout(() => resolve({ default: Inner }), 20)));
  return {
    PANEL_ENTRIES: {
      files: {
        icon: FolderClosed,
        name: "panel.files.name",
        toggle: { show: "titleBar.files.show", hide: "titleBar.files.hide" },
        label: () => "Files",
        Content: (props: { switching: boolean }) => (
          <Suspense fallback={null}>
            <Loaded {...props} />
          </Suspense>
        ),
      },
    },
  };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const noKeys = { onEscape: () => {}, onCycleRegion: () => {}, onMoveHistory: () => {}, onMoveConsoleSession: () => {} };

it("does not slide in the first panel of a pane that was just shown, however late its code arrives", async () => {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (project: string) =>
    act(() =>
      root.render(
        <PanelStack
          owner={{ kind: "project", project }}
          project={{ id: project, name: project } as Project}
          active
          selectedSessionId={undefined}
          focusTerminal={() => {}}
          peek={undefined}
          peekKind={undefined}
          frameKeys={noKeys}
          onDismiss={() => {}}
          maximizeArea={{ current: null }}
        />,
      ),
    );
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  render("p");
  // The stack has mounted and its effects have run; the code has not arrived.
  expect(container.querySelector("button[data-switching]")).toBeNull();
  for (let i = 0; i < 20 && !container.querySelector("button[data-switching]"); i++) await act(async () => void (await new Promise((resolve) => setTimeout(resolve, 20))));
  expect(container.querySelector("button[data-switching]")).not.toBeNull();
  expect(container.querySelector("button[data-switching]")!.getAttribute("data-switching")).toBe("false");
  // Another owner slides in, and so does the first one on the way back.
  const flag = async () => {
    for (let i = 0; i < 20 && !container.querySelector("button[data-switching]"); i++) await act(async () => void (await new Promise((resolve) => setTimeout(resolve, 20))));
    return container.querySelector("button[data-switching]")!.getAttribute("data-switching");
  };
  render("q");
  expect(await flag()).toBe("true");
  render("p");
  expect(await flag()).toBe("true");
});
