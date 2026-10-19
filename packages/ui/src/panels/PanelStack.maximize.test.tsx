// @vitest-environment jsdom
import { FolderClosed } from "lucide-react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { PanelHeader } from "../components/PanelHeader";
import type { PanePeek } from "../layout/usePaneToggles";
import type { Project } from "../protocol";
import { PanelStack } from "./PanelStack";
import type { PanelKind } from "./stackState";
import { usePanelStacks } from "./usePanelStacks";

// A second kind of panel, the maximizable one, stands in for the Terminal panel.
vi.mock("./stackState", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./stackState")>();
  return { ...actual, OWNER_KINDS: { ...actual.OWNER_KINDS, project: ["files", "fake"] } };
});
vi.mock("./panelRegistry", () => {
  const entry = (name: string, maximizable: boolean) => ({
    icon: FolderClosed,
    name: "panel.files.name",
    toggle: { show: "titleBar.files.show", hide: "titleBar.files.hide" },
    label: () => name,
    maximizable,
    Content: () => (
      <>
        <PanelHeader title={name} />
        <button type="button" id={`inside-${name}`}>inside</button>
      </>
    ),
  });
  return { PANEL_ENTRIES: { files: entry("files", false), fake: entry("fake", true) } };
});

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** The stack beside the agent terminal's area, which the maximized card covers, and below a top
 * bar's control, which it does not. */
function mount({ open, peek }: { open: string[]; peek?: PanePeek }) {
  const area: { current: HTMLDivElement | null } = { current: null };
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
        <button type="button" id="top-bar">top bar</button>
        <div>
          <div ref={area}>
            <button type="button" id="agent">agent terminal</button>
          </div>
          <PanelStack
            owner={{ kind: "project", project: "p" }}
            project={{ id: "p", name: "p" } as Project}
            active
            selectedSessionId={undefined}
            focusTerminal={() => {}}
            peek={peek}
            peekKind={peek ? ("fake" as PanelKind) : undefined}
            frameKeys={{ onEscape: () => {}, onCycleRegion: () => {}, onMoveHistory: () => {}, onMoveConsoleSession: () => {} }}
            onDismiss={() => {}}
            maximizeArea={area}
          />
        </div>
      </>,
    ),
  );
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => latest.setStack("project", { open: ["files"], shares: {} }));
  });
  act(() => latest.setStack("project", { open, shares: {} }));
  const press = (selector: string) => act(() => void container.querySelector<HTMLButtonElement>(selector)!.click());
  return { container, stack: () => latest.stacks.project, press };
}

it("offers maximize on a maximizable kind open alone", () => {
  const { container } = mount({ open: ["fake"] });
  expect(container.querySelector("#panel-fake [aria-label='Maximize panel']")).not.toBeNull();
});

it("offers maximize only on a maximizable kind, never on the others beside it", () => {
  const { container } = mount({ open: ["files", "fake"] });
  expect(container.querySelector("#panel-files [aria-label='Maximize panel']")).toBeNull();
  expect(container.querySelector("#panel-fake [aria-label='Maximize panel']")).not.toBeNull();
});

it("offers no maximize on a card that only floats in", () => {
  const { container } = mount({ open: [], peek: { active: true, reveal: () => {}, keep: () => {}, leave: () => {} } });
  expect(container.querySelector("#panel-fake")).not.toBeNull();
  expect(container.querySelector("[aria-label='Maximize panel']")).toBeNull();
});

it("lays the maximized card over the agent terminal's area, on a backdrop, the others left in the column, and puts it back as it was", () => {
  const { container, stack, press } = mount({ open: ["files", "fake"] });
  const before = stack();
  expect(container.querySelector("[role=separator]")).not.toBeNull();
  press("#panel-fake [aria-label='Maximize panel']");
  expect(stack().maximized).toBe("fake");
  const card = container.querySelector<HTMLElement>("#panel-fake")!;
  expect(card.hasAttribute("data-panel-maximized")).toBe(true);
  expect(card.classList.contains("absolute")).toBe(true);
  expect(card.style.top).toContain("--peek-gap");
  const files = container.querySelector<HTMLElement>("#panel-files")!;
  expect(files.getAttribute("data-region")).toBe("panel-files");
  expect(files.style.flex).toBe("1 1 0px");
  expect(container.querySelector("[aria-hidden=true].absolute")).not.toBeNull();
  // The column has no pair left to resize.
  expect(container.querySelector("[role=separator]")).toBeNull();

  press("#panel-fake [aria-label='Restore panel size']");
  expect(stack()).toEqual({ ...before, maximized: undefined });
  expect(card.hasAttribute("data-panel-maximized")).toBe(false);
  expect(card.classList.contains("absolute")).toBe(false);
  expect(container.querySelector("[aria-hidden=true].absolute")).toBeNull();
  expect(container.querySelector("[role=separator]")).not.toBeNull();
});

it("hands focus on the agent terminal's area to the card it is about to be covered by", () => {
  const { press } = mount({ open: ["files", "fake"] });
  // The control that maximizes is pressed without taking focus, while focus sits in the agent terminal.
  document.getElementById("agent")!.focus();
  press("#panel-fake [aria-label='Maximize panel']");
  expect(document.activeElement?.closest("[data-panel-card]")?.id).toBe("panel-fake");
});

it("leaves focus that is not in the covered area where it is", () => {
  const { press } = mount({ open: ["files", "fake"] });
  document.getElementById("inside-files")!.focus();
  press("#panel-fake [aria-label='Maximize panel']");
  expect(document.activeElement?.id).toBe("inside-files");
  press("#panel-fake [aria-label='Restore panel size']");
  document.getElementById("top-bar")!.focus();
  press("#panel-fake [aria-label='Maximize panel']");
  expect(document.activeElement?.id).toBe("top-bar");
});
