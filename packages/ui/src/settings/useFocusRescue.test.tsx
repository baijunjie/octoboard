// @vitest-environment jsdom
import { act, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { framesAsTimers, settleFrames, withModality } from "../components/dialog/dialogTestSupport";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { useFocusRescue } from "./useFocusRescue";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let change: (state: "shown" | "hidden" | "removed") => void;

type TabState = "shown" | "hidden" | "removed";

function Dialog({ tab }: { tab: TabState }) {
  const [state, setState] = useState<"shown" | "hidden" | "removed">("shown");
  useEffect(() => { change = (next) => act(() => setState(next)); }, []);
  const dialog = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  useFocusRescue(
    () => dialog.current,
    () => search.current,
    () => dialog.current?.querySelector<HTMLElement>(tab === "shown" ? '[role="tab"][aria-selected="true"]' : "[data-close]") ?? null,
    () => dialog.current?.querySelector<HTMLElement>("[data-close]") ?? null,
  );
  return (
    <div>
      <div ref={dialog}>
        <button data-close>close</button>
        {tab !== "removed" && <div hidden={tab === "hidden"}><button role="tab" aria-selected="true">tab</button></div>}
        <input ref={search} aria-label="search" />
        {state !== "removed" && <div hidden={state === "hidden"}><button data-row>row</button></div>}
        <button>other</button>
      </div>
      <button>outside</button>
    </div>
  );
}

function mount(tab: TabState = "shown") {
  framesAsTimers();
  const host = document.body.appendChild(document.createElement("div"));
  const root = createRoot(host);
  onTestFinished(() => { act(() => root.unmount()); host.remove(); });
  act(() => root.render(<Dialog tab={tab} />));
  return {
    setTab: (next: TabState) => act(() => root.render(<Dialog tab={next} />)),
    row: () => host.querySelector<HTMLElement>("[data-row]")!,
    search: () => host.querySelector<HTMLInputElement>("input")!,
    other: () => Array.from(host.querySelectorAll<HTMLElement>("button")).find((button) => button.textContent === "other")!,
    tab: () => host.querySelector<HTMLElement>('[role="tab"]')!,
    close: () => host.querySelector<HTMLElement>("[data-close]")!,
  };
}

it("moves focus to the field, with the keyboard ring, when the element holding it is hidden", async () => {
  const page = mount();
  act(() => page.row().focus());
  // A hardware key makes the ring follow it; the modality is then virtual, as after a press that came from a script.
  document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  withModality("virtual");
  expect(getInteractionModality()).toBe("virtual");
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(page.search());
  expect(getInteractionModality()).toBe("keyboard");
});

it("moves focus to the field when the element holding it is removed", async () => {
  const page = mount();
  act(() => page.row().focus());
  change("removed");
  await settleFrames();
  expect(document.activeElement).toBe(page.search());
});

it("leaves focus alone when it is on an element that stays shown", async () => {
  const page = mount();
  act(() => page.other().focus());
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(page.other());
});

it("leaves focus alone when it had already gone elsewhere in the dialog", async () => {
  const page = mount();
  act(() => page.row().focus());
  act(() => page.other().focus());
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(page.other());
});

it("waits for focus to fall when the element is hidden while a popover still holds it", async () => {
  const page = mount();
  const popover = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => popover.remove());
  act(() => page.row().focus());
  act(() => popover.focus());
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(popover);
  act(() => popover.remove());
  await settleFrames();
  expect(document.activeElement).toBe(page.search());
});

it("drops the wait when focus never falls, so a later render does not rescue", async () => {
  const page = mount();
  const popover = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => popover.remove());
  act(() => page.row().focus());
  act(() => popover.focus());
  change("hidden");
  await settleFrames(40);
  act(() => popover.remove());
  change("removed");
  await settleFrames();
  expect(document.activeElement).not.toBe(page.search());
});

function tap() {
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "touch" }));
  onTestFinished(() => { document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse" })); });
}

it("hands focus to the selected tab, not the field, after a tap", async () => {
  const page = mount();
  act(() => page.row().focus());
  tap();
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(page.tab());
});

it("hands focus to Close after a tap when the tab is hidden", async () => {
  const page = mount("hidden");
  act(() => page.row().focus());
  tap();
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(page.close());
});

it("still rescues when the focus scope's fallback to the first control lands during the wait", async () => {
  const page = mount();
  const popover = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => popover.remove());
  act(() => page.row().focus());
  act(() => popover.focus());
  change("hidden");
  await settleFrames();
  act(() => popover.remove());
  // The focus scope takes focus to the dialog's first control as the popover goes.
  act(() => page.close().focus());
  await settleFrames();
  expect(document.activeElement).toBe(page.search());
});

it("leaves alone a handoff that focuses another control in the dialog during the wait", async () => {
  const page = mount();
  const popover = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => popover.remove());
  act(() => page.row().focus());
  act(() => popover.focus());
  change("hidden");
  await settleFrames();
  act(() => popover.remove());
  act(() => page.tab().focus());
  await settleFrames();
  expect(document.activeElement).toBe(page.tab());
});

it.each(["hidden", "removed"] as const)("moves focus on to Close when the tab it was handed to is %s by a later render", async (later) => {
  const page = mount();
  act(() => page.row().focus());
  tap();
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(page.tab());
  page.setTab(later);
  await settleFrames();
  expect(document.activeElement).toBe(page.close());
});

it("does not rescue again, or focus Close again, on a later render once Close holds the rescued focus", async () => {
  const page = mount("hidden");
  act(() => page.row().focus());
  tap();
  change("hidden");
  await settleFrames();
  expect(document.activeElement).toBe(page.close());
  const refocus = vi.spyOn(page.close(), "focus");
  change("removed");
  page.setTab("hidden");
  await settleFrames();
  expect(refocus).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(page.close());
});
