// @vitest-environment jsdom
import { Folder } from "lucide-react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { PlatformProvider } from "../platform/react";
import { TitleBar } from "./TitleBar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const toggle = (open: boolean, onPress: () => void) => ({
  kind: "files",
  label: open ? "Hide files panel" : "Show files panel",
  Icon: Folder,
  open,
  regionId: open ? "panel-files" : undefined,
  onPress,
  onHoverChange: () => {},
});

function mount(props: Partial<React.ComponentProps<typeof TitleBar>>) {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (next: Partial<React.ComponentProps<typeof TitleBar>>) => act(() =>
    root.render(
      <PlatformProvider value={{ kind: "browser" }}>
        <TitleBar
          sidebarShown
          hasSidebar
          sidebarSpan={280}
          sidebarEasing={false}
          onToggleSidebar={() => {}}
          onSidebarToggleEnter={() => {}}
          onSidebarToggleLeave={() => {}}
          asideSpan={420}
          canGoBack={false}
          canGoForward={false}
          onBack={() => {}}
          onForward={() => {}}
          focusTerminal={() => {}}
          {...next}
        />
      </PlatformProvider>,
    ),
  );
  render(props);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { container, render };
}

it("has a toggle per panel kind and none of its own for the pane, labelled with what it does and carrying its expanded state", () => {
  const onPress = vi.fn();
  const { container } = mount({ panelToggles: [toggle(true, onPress)] });
  const buttons = Array.from(container.querySelectorAll<HTMLButtonElement>(".title-bar-end button"));
  expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(["Hide files panel"]);
  expect(buttons[0].getAttribute("aria-expanded")).toBe("true");
  act(() => void buttons[0].click());
  expect(onPress).toHaveBeenCalledOnce();
  expect(buttons[0].getAttribute("aria-controls")).toBe("panel-files");
  // Counted as part of the floating pane, so the pointer resting on it keeps the pane up.
  expect(buttons[0].getAttribute("data-pane-toggle")).toBe("aside-pane");
});

it("says a closed panel's toggle shows it, and has no toggles while nothing owns the pane", () => {
  const closed = mount({ panelToggles: [toggle(false, () => {})] });
  const button = closed.container.querySelector(".title-bar-end button")!;
  expect(button.getAttribute("aria-label")).toBe("Show files panel");
  expect(button.getAttribute("aria-expanded")).toBe("false");
  expect(button.hasAttribute("aria-controls")).toBe(false);
  const none = mount({});
  expect(none.container.querySelectorAll(".title-bar-end button")).toHaveLength(0);
});

it("hands focus to the terminal, with the focus ring, when the toggle that holds it goes away", () => {
  const focusTerminal = vi.fn();
  const onPress = vi.fn();
  const { container, render } = mount({ panelToggles: [toggle(false, onPress)], focusTerminal });
  act(() => container.querySelector<HTMLElement>(".title-bar-end button")!.focus());
  act(() => setInteractionModality("pointer"));
  render({ panelToggles: [], focusTerminal });
  expect(focusTerminal).toHaveBeenCalledOnce();
  expect(getInteractionModality()).toBe("keyboard");
});

it("leaves focus alone when a toggle goes away while focus is elsewhere", () => {
  const focusTerminal = vi.fn();
  const { render } = mount({ panelToggles: [toggle(false, () => {})], focusTerminal });
  render({ panelToggles: [], focusTerminal });
  expect(focusTerminal).not.toHaveBeenCalled();
});
