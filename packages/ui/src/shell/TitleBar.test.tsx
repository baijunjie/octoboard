// @vitest-environment jsdom
import { Folder } from "lucide-react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { sessionOf } from "../fixtures/builders";
import { currentLanguage, setLanguage } from "../i18n/language";
import { PlatformProvider } from "../platform/react";
import { TitleBar } from "./TitleBar";
import { BROWSER_HOST } from "../test/hosts";
import { TestHost } from "../test/TestHost";
import { testDaemon } from "../test/testDaemon";
import { tap, press as pressMark } from "../components/hint/tooltipTestSupport";
import { framesAsTimers, settleFrames, trackMountRefocus } from "../components/dialog/dialogTestSupport";

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
      <PlatformProvider value={{ kind: "browser", host: BROWSER_HOST }}>
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
  act(() => {
    // Reset touchInput's last input as well: an earlier touch press can leave its module state set to touch.
    document.body.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
    setInteractionModality("pointer");
  });
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

const RECOVERED = "Recovered terminal display";
const BODY = "The display was rebuilt after a connection interruption. Earlier output and parts of the screen may be missing.";
const recoveredMark = (container: HTMLElement) => container.querySelector<HTMLButtonElement>(`button[aria-label="${RECOVERED}"]`);
const dialog = () => document.querySelector('[role="dialog"]');

function mountWithSession(session: ReturnType<typeof sessionOf>, focusTerminal: () => void = () => {}) {
  const daemon = testDaemon({});
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  const render = (next: typeof session) => act(() => root.render(
    <TestHost daemon={daemon}>
      <TitleBar sidebarShown hasSidebar sidebarSpan={280} sidebarEasing={false} onToggleSidebar={() => {}} onSidebarToggleEnter={() => {}} onSidebarToggleLeave={() => {}} asideSpan={420} canGoBack={false} canGoForward={false} onBack={() => {}} onForward={() => {}} focusTerminal={focusTerminal} selectedSession={next} />
    </TestHost>,
  ));
  render(session);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { container, render };
}

it("draws no note beside the title while the display is exact or the flag is absent", () => {
  const absent = mountWithSession(sessionOf("s", "c", undefined, "Work", "working"));
  expect(recoveredMark(absent.container)).toBeNull();
  const off = mountWithSession(sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: false }));
  expect(recoveredMark(off.container)).toBeNull();
});

it("names a session whose process cannot be reached Reconnecting in its status slot, whatever its stored status", () => {
  const session = sessionOf("s", "c", undefined, "Work", "working", { holder_link: "pending" });
  const { container, render } = mountWithSession(session);
  const names = () => Array.from(container.querySelectorAll("[aria-label]"), (element) => element.getAttribute("aria-label"));
  expect(names()).toContain("Reconnecting");
  render({ ...session, holder_link: undefined });
  expect(names()).not.toContain("Reconnecting");
  expect(names()).toContain("Working");
});

it("draws the note right after the title and before the status icon, and keeps it across a redraw while the flag is true", () => {
  const session = sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: true });
  const { container, render } = mountWithSession(session);
  const mark = recoveredMark(container)!;
  expect(mark.querySelector("svg.lucide-info")).not.toBeNull();
  const title = Array.from(container.querySelectorAll("span")).find((span) => span.textContent === "Work")!;
  expect(title.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  const icon = container.querySelector("[data-status-mark]")!.parentElement!.lastElementChild!;
  expect(icon).not.toBe(mark);
  render({ ...session, status: "idle" });
  render({ ...session, status: "working" });
  expect(recoveredMark(container)).toBe(mark);
  render({ ...session, screen_inexact: false });
  expect(recoveredMark(container)).toBeNull();
});

it("opens the explanation from the keyboard and from a touch", () => {
  const { container } = mountWithSession(sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: true }));
  const mark = recoveredMark(container)!;
  act(() => mark.focus());
  tap(mark, "Enter");
  expect(dialog()?.getAttribute("aria-label")).toBe(RECOVERED);
  expect(dialog()?.textContent).toBe(BODY);
  act(() => void dialog()!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(dialog()).toBeNull();
  pressMark(mark, "touch");
  expect(dialog()?.textContent).toBe(BODY);
});

it("words the note in the interface language, with the mark in the title's own flow under right-to-left", () => {
  const before = currentLanguage();
  setLanguage("ar");
  onTestFinished(() => setLanguage(before));
  const { container } = mountWithSession(sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: true }));
  const mark = container.querySelector<HTMLButtonElement>("button[data-status-mark]")!;
  expect(mark.getAttribute("aria-label")).toBe("تمت استعادة عرض الوحدة الطرفية");
  // Placed by order in the flow, not by a physical offset, so it follows the title to the right side.
  expect(mark.className).not.toMatch(/(^|\s)(ml|mr|left|right|pl|pr)-/);
});

it("hands focus to the terminal at once, with the focus ring, when the note goes while only its mark holds focus", () => {
  framesAsTimers();
  const session = sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: true });
  const landing = document.body.appendChild(document.createElement("textarea"));
  onTestFinished(() => landing.remove());
  const focusTerminal = vi.fn(() => landing.focus());
  const { container, render } = mountWithSession(session, focusTerminal);
  act(() => recoveredMark(container)!.focus());
  // A mouse press just before, which an earlier touch in the file would otherwise outlast.
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
  act(() => setInteractionModality("pointer"));
  render({ ...session, screen_inexact: false });
  // No frame has passed: the handoff is in the commit that removed the mark.
  expect(focusTerminal).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(landing);
  expect(getInteractionModality()).toBe("keyboard");
});

async function openExplanation(container: HTMLElement) {
  act(() => recoveredMark(container)!.focus());
  tap(recoveredMark(container)!, "Enter");
  expect(dialog()).not.toBeNull();
  const mountSettled = trackMountRefocus();
  await settleFrames();
  await mountSettled();
  act(() => (dialog() as HTMLElement).focus());
}

it("hands focus to the terminal from the open explanation only after its own restore, without losing the landing", async () => {
  framesAsTimers();
  const session = sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: true });
  const landing = document.body.appendChild(document.createElement("textarea"));
  onTestFinished(() => landing.remove());
  const focusTerminal = vi.fn(() => landing.focus());
  const { container, render } = mountWithSession(session, focusTerminal);
  await openExplanation(container);
  render({ ...session, screen_inexact: false });
  expect(focusTerminal).not.toHaveBeenCalled();
  await settleFrames();
  expect(document.activeElement).toBe(landing);
  expect(focusTerminal).toHaveBeenCalledOnce();
  expect(getInteractionModality()).toBe("keyboard");
});

it("does not take focus back if another control claimed it while the open explanation was removed", async () => {
  framesAsTimers();
  const session = sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: true });
  const focusTerminal = vi.fn();
  const { container, render } = mountWithSession(session, focusTerminal);
  const outside = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => outside.remove());
  await openExplanation(container);
  render({ ...session, screen_inexact: false });
  act(() => outside.focus());
  await settleFrames();
  expect(document.activeElement).toBe(outside);
  expect(focusTerminal).not.toHaveBeenCalled();
});

it("leaves focus alone when the note goes while focus is elsewhere", () => {
  const session = sessionOf("s", "c", undefined, "Work", "working", { screen_inexact: true });
  const focusTerminal = vi.fn();
  const { container, render } = mountWithSession(session, focusTerminal);
  const outside = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => outside.remove());
  outside.focus();
  expect(container.contains(outside)).toBe(false);
  render({ ...session, screen_inexact: false });
  expect(focusTerminal).not.toHaveBeenCalled();
});
