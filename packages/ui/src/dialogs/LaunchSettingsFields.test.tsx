// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { LaunchSettingsFields } from "./LaunchSettingsFields";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Longer than `StatusAnnouncer`'s first write delay. */
const PAST_FIRST_WRITE = 150;

function mount() {
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type === "list_agent_models") throw new Error("Fixture has no models");
    return { type: "ack" };
  });
  const daemon = { store: createStateStore({}), request, toastError: vi.fn(), toastNotice: vi.fn() } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <LaunchSettingsFields agent="claude" account={null} model={null} effort={null} onModelChange={() => {}} onEffortChange={() => {}} note="Note" />
      </DaemonProvider>,
    ),
  );
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  const toggle = () => act(() => [...container.querySelectorAll("button")].find((button) => button.textContent === "Advanced")!.click());
  /** The status regions assistive technology can reach, with what each says. */
  const exposedStatuses = () =>
    [...container.querySelectorAll('[role="status"]')].filter((region) => !region.closest("[hidden], [inert], [aria-hidden='true']")).map((region) => region.textContent);
  const wait = (ms: number) => act(() => new Promise((resolve) => setTimeout(resolve, ms)));
  return { toggle, exposedStatuses, wait, request };
}

// The failure kept from the first opening comes back with the body; written in that same render it
// would go unsaid, so the region is mounted anew with each opening and writes it after its delay.
// The list is asked for again on each opening, and the same answer writes nothing new.
it("says the model list's failure again each time the section is opened, once, after the region is there", async () => {
  const { toggle, exposedStatuses, wait, request } = mount();
  toggle();
  expect(exposedStatuses()).toEqual([""]);
  await wait(PAST_FIRST_WRITE);
  expect(exposedStatuses()).toEqual(["Fixture has no models"]);

  toggle();
  expect(exposedStatuses().filter(Boolean)).toEqual([]);

  toggle();
  expect(exposedStatuses()).toEqual([""]);
  const region = [...document.querySelectorAll('[role="status"]')].find((element) => !element.closest("[hidden]"))!;
  const writes: string[] = [];
  const observer = new MutationObserver(() => writes.push(region.textContent ?? ""));
  observer.observe(region, { childList: true, characterData: true, subtree: true });
  onTestFinished(() => observer.disconnect());
  await wait(PAST_FIRST_WRITE * 2);
  expect(exposedStatuses()).toEqual(["Fixture has no models"]);
  expect(writes).toEqual(["Fixture has no models"]);
  expect(request).toHaveBeenCalledTimes(2);
});
