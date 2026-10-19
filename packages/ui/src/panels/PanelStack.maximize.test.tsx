// @vitest-environment jsdom
import { FolderClosed } from "lucide-react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { PanelHeader } from "../components/PanelHeader";
import type { Project } from "../protocol";
import { PanelStack } from "./PanelStack";
import { usePanelStacks } from "./usePanelStacks";

// A second kind of panel stands in for the real ones that do not exist yet.
vi.mock("./stackState", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./stackState")>();
  return { ...actual, OWNER_KINDS: { ...actual.OWNER_KINDS, project: ["files", "fake"] } };
});
vi.mock("./panelRegistry", () => {
  const entry = (name: string) => ({
    icon: FolderClosed,
    name: "panel.files.name",
    toggle: { show: "titleBar.files.show", hide: "titleBar.files.hide" },
    label: () => name,
    Content: () => (
      <>
        <PanelHeader title={name} />
        <button type="button" id={`inside-${name}`}>inside</button>
      </>
    ),
  });
  return { PANEL_ENTRIES: { files: entry("files"), fake: entry("fake") } };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

it("hands focus to the card left on screen when maximizing hides the card that holds it", () => {
  let latest!: ReturnType<typeof usePanelStacks>;
  function Probe() {
    latest = usePanelStacks();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <>
        <Probe />
        <PanelStack
          owner={{ kind: "project", project: "p" }}
          project={{ id: "p", name: "p" } as Project}
          active
          selectedSessionId={undefined}
          focusTerminal={() => {}}
          peek={undefined}
          peekKind={undefined}
          frameKeys={{ onEscape: () => {}, onCycleRegion: () => {}, onMoveHistory: () => {}, onMoveConsoleSession: () => {} }}
          onDismiss={() => {}}
        />
      </>,
    ),
  );
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => latest.setStack("project", { open: ["files"], shares: {} }));
  });
  act(() => latest.setStack("project", { open: ["files", "fake"], shares: {} }));
  expect(container.querySelectorAll("[data-panel-card]")).toHaveLength(2);
  // The control that maximizes the first card is pressed without taking focus, while focus sits in the second.
  document.getElementById("inside-fake")!.focus();
  act(() => void container.querySelector<HTMLButtonElement>("#panel-files [aria-label='Maximize panel']")!.click());
  expect(latest.stacks.project.maximized).toBe("files");
  const holder = document.activeElement;
  expect(holder?.closest("[data-panel-card]")?.id).toBe("panel-files");
  expect(holder?.closest("[data-hidden]")).toBeNull();
});
