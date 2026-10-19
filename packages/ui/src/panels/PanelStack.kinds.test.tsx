// @vitest-environment jsdom
import { FolderClosed } from "lucide-react";
import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { PanelHeader } from "../components/PanelHeader";
import type { PanePeek } from "../layout/usePaneToggles";
import type { Project } from "../protocol";
import { PanelStack } from "./PanelStack";
import { usePanelStacks } from "./usePanelStacks";

// Two kinds of the one owner, each reading the switching flag once as it mounts, as the real ones do.
vi.mock("./stackState", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./stackState")>();
  return { ...actual, OWNER_KINDS: { ...actual.OWNER_KINDS, project: ["files", "fake"] } };
});
const mounted = vi.hoisted(() => [] as string[]);
vi.mock("./panelRegistry", () => {
  const entry = (name: string) => ({
    icon: FolderClosed,
    name: "panel.files.name",
    toggle: { show: "titleBar.files.show", hide: "titleBar.files.hide" },
    label: () => name,
    Content: ({ switching }: { switching: boolean }) => {
      const [atMount] = useState(() => {
        mounted.push(name);
        return switching;
      });
      return (
        <>
          <PanelHeader title={name} />
          <button type="button" data-kind={name} data-switching={String(atMount)}>inside</button>
        </>
      );
    },
  });
  return { PANEL_ENTRIES: { files: entry("files"), fake: entry("fake") } };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount() {
  mounted.length = 0;
  let latest!: ReturnType<typeof usePanelStacks>;
  function Probe() {
    latest = usePanelStacks();
    return null;
  }
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  let project = "p";
  let peek: PanePeek | undefined;
  const render = () =>
    act(() =>
      root.render(
        <>
          <Probe />
          <PanelStack
            owner={{ kind: "project", project }}
            project={{ id: project, name: project } as Project}
            active
            selectedSessionId={undefined}
            focusTerminal={() => {}}
            peek={peek}
            peekKind="files"
            frameKeys={{ onEscape: () => {}, onCycleRegion: () => {}, onMoveHistory: () => {}, onMoveConsoleSession: () => {} }}
            onDismiss={() => {}}
            maximizeArea={{ current: null }}
          />
        </>,
      ),
    );
  render();
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => latest.setStack("project", { open: ["files"], shares: {} }));
  });
  return {
    container,
    setOpen: (open: string[]) => act(() => latest.setStack("project", { open, shares: {} })),
    switchTo: (id: string) => {
      project = id;
      render();
    },
    /** The stack as the pane hands it over once no panel is open: floating in (`active`) or not. */
    undock: (active: boolean) => {
      peek = { active, reveal: () => {}, keep: () => {}, leave: () => {} };
      render();
    },
    flag: (kind: string) => container.querySelector(`[data-kind=${kind}]`)?.getAttribute("data-switching"),
  };
}

it("slides a panel in only when the owner changes under its own card, not when it is opened beside another after one", () => {
  const stack = mount();
  stack.setOpen(["files", "fake"]);
  stack.switchTo("q");
  expect(stack.flag("files")).toBe("true");
  expect(stack.flag("fake")).toBe("true");
  // Closed and opened again under the owner it now has: it opens, it does not switch.
  stack.setOpen(["fake"]);
  stack.setOpen(["files", "fake"]);
  expect(stack.flag("files")).toBe("false");
});

it("mounts no other card once the last docked panel closes, and shows the hovered kind only when the pane next floats in", () => {
  const stack = mount();
  stack.setOpen(["fake"]);
  mounted.length = 0;
  stack.setOpen([]);
  stack.undock(false);
  expect(stack.container.querySelector("[data-panel-card]")).toBeNull();
  expect(mounted).toEqual([]);
  stack.undock(true);
  expect(stack.flag("files")).toBe("false");
  // Floating out keeps the card it floated in with until the pane is gone.
  stack.undock(false);
  expect(stack.container.querySelector("[data-kind=files]")).not.toBeNull();
});
