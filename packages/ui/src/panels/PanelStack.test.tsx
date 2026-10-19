// @vitest-environment jsdom
import { FolderOpen } from "lucide-react";
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import type { Project } from "../protocol";
import { PanelHeader } from "../components/PanelHeader";
import { PanelStack } from "./PanelStack";
import { usePanelStacks } from "./usePanelStacks";

vi.mock("./panelRegistry", () => ({
  PANEL_ENTRIES: {
    files: {
      icon: FolderOpen,
      toggle: { show: "titleBar.files.show", hide: "titleBar.files.hide" },
      label: (_t: unknown, project: Project) => `Files of ${project.name}`,
      // Reads the flag once, as the real browser does.
      Content: ({ project, switching }: { project: Project; switching: boolean }) => {
        const [atMount] = useState(switching);
        return (
          <>
            <PanelHeader title={project.name} />
            <button type="button" data-switching={String(atMount)}>inside</button>
          </>
        );
      },
    },
  },
}));

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const noKeys = { onEscape: () => {}, onCycleRegion: () => {}, onMoveHistory: () => {}, onMoveConsoleSession: () => {} };

function mount({ peekKind }: { peekKind?: "files" } = {}) {
  const focusTerminal = vi.fn();
  const onDismiss = vi.fn();
  let latest!: ReturnType<typeof usePanelStacks>;
  function Probe() {
    latest = usePanelStacks();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let current = "p";
  const tree = (withStack: boolean) => (
    <>
      <Probe />
      {withStack && (
        <PanelStack
          owner={{ kind: "project", project: current }}
          project={{ id: current, name: `Proj ${current}` } as Project}
          active
          selectedSessionId={undefined}
          focusTerminal={focusTerminal}
          peek={peekKind ? { active: true, reveal: () => {}, keep: () => {}, leave: () => {} } : undefined}
          peekKind={peekKind}
          frameKeys={noKeys}
          onDismiss={onDismiss}
          maximizeArea={{ current: null }}
        />
      )}
    </>
  );
  act(() => root.render(tree(true)));
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => latest.setStack("project", { open: ["files"], shares: {} }));
  });
  return {
    container,
    focusTerminal,
    onDismiss,
    stack: () => latest.stacks.project,
    setProject: (open: string[]) => act(() => latest.setStack("project", { open, shares: {} })),
    switchTo: (id: string) => act(() => { current = id; root.render(tree(true)); }),
    unmountStack: () => act(() => root.render(tree(false))),
  };
}

it("hands focus to the terminal when the card holding it goes away, as the top bar's toggle closes one", () => {
  const { container, focusTerminal, unmountStack } = mount();
  container.querySelector<HTMLButtonElement>("button:not([aria-label])")!.focus();
  unmountStack();
  expect(focusTerminal).toHaveBeenCalledOnce();
});

it("shows the open panel as a named region and F6 stop, with a header that runs on with the other panes' despite the gap above the card", () => {
  const { container } = mount();
  const card = container.querySelector<HTMLElement>("[data-region=panel-files]")!;
  expect(card.getAttribute("role")).toBe("region");
  expect(card.getAttribute("aria-label")).toBe("Files of Proj p");
  expect(card.id).toBe("panel-files");
  expect(card.querySelector("h2")!.textContent).toBe("Proj p");
  expect(card.querySelector(".h-\\(--pane-header-height-floating\\)")).not.toBeNull();
});

it("styles the card as the sidebar's floating card, with the same header height", () => {
  const { container } = mount();
  const card = container.querySelector<HTMLElement>("[data-region=panel-files]")!;
  for (const name of ["rounded-xl", "overflow-hidden", "bg-panel-sidebar", "panel-edge", "shadow-(--panel-card-shadow)"]) expect(card.classList.contains(name), name).toBe(true);
  expect(card.className).not.toContain("panel-shadow");
});

it("offers no maximize on a kind that is not maximizable, and closing the last panel leaves the owner's stack empty", () => {
  const { container, stack } = mount();
  expect(container.querySelector("[aria-label='Maximize panel']")).toBeNull();
  act(() => void container.querySelector<HTMLButtonElement>("[aria-label='Close panel']")!.click());
  expect(stack().open).toEqual([]);
});

it("shows the panel whose toggle is hovered while none is open, and closing it only dismisses the floating pane", () => {
  const { container, setProject, stack, onDismiss } = mount({ peekKind: "files" });
  setProject([]);
  expect(container.querySelector("[data-region=panel-files]")).not.toBeNull();
  act(() => void container.querySelector<HTMLButtonElement>("[aria-label='Close panel']")!.click());
  expect(onDismiss).toHaveBeenCalledOnce();
  expect(stack().open).toEqual([]);
});

it("tells a panel it appears in a pane already on screen only when the owner changes under the stack", () => {
  const { container, switchTo } = mount();
  const flag = () => container.querySelector("button[data-switching]")!.getAttribute("data-switching");
  // It mounted with the stack, which docked or floated in with it.
  expect(flag()).toBe("false");
  switchTo("q");
  expect(container.querySelector("h2")!.textContent).toBe("Proj q");
  expect(flag()).toBe("true");
});
