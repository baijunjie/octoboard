// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { openDetails } from "../components/detailsTestSupport";
import { DaemonRequestError } from "../daemon-client";
import { isolate } from "../i18n/isolate";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { EffortPicker, LaunchSettingsFields, ModelPicker, type LaunchModels } from "./LaunchSettingsFields";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Longer than `StatusAnnouncer`'s first write delay. */
const PAST_FIRST_WRITE = 150;

function mount(unavailable?: import("../protocol").ModelsUnavailable | Error) {
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type === "list_agent_models") {
      if (unavailable instanceof Error) throw unavailable;
      if (unavailable) return { type: "agent_models", models: [], unavailable };
      throw new Error("Fixture has no models");
    }
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
  return { toggle, exposedStatuses, wait, request, container };
}

// The failure kept from the first opening comes back with the body; written in that same render it
// would go unsaid, so the region is mounted anew with each opening and writes it after its delay.
// The list is asked for again on each opening, and the same answer writes nothing new.
it("says the model list's failure again each time the section is opened, once, after the region is there", async () => {
  const { toggle, exposedStatuses, wait, request } = mount();
  toggle();
  expect(exposedStatuses()).toEqual([""]);
  await wait(PAST_FIRST_WRITE);
  // A thrown error is a failure the application has no words for: the region says the Alert's generic
  // sentence, and the error's own message stays behind the Alert's Details.
  expect(exposedStatuses()).toEqual(["Something unexpected went wrong."]);
  expect(document.body.textContent).not.toContain("Fixture has no models");

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
  expect(exposedStatuses()).toEqual(["Something unexpected went wrong."]);
  expect(writes).toEqual(["Something unexpected went wrong."]);
  expect(request).toHaveBeenCalledTimes(2);
});

it("keeps model diagnostics outside the field description while announcing only the summary sentence once", async () => {
  const detail = "error: models unavailable\n    /tmp/مراجعة";
  const { toggle, exposedStatuses, wait, container } = mount({ code: "models_command_failed", params: { agent: "Codex", detail }, message: "fallback" });
  toggle();
  await wait(PAST_FIRST_WRITE);
  const description = [...container.querySelectorAll('[data-slot="alert-description"]')].find((node) => node.textContent === "Codex could not list its models.")!;
  expect(description.textContent).toBe("Codex could not list its models.");
  expect(description.querySelector("pre,div,p")).toBeNull();
  // The whole failure is one Alert: the diagnostics are behind its Details button, in a popover of their own.
  const alert = description.closest('[data-slot="alert-root"]')!;
  expect(container.querySelector("pre")).toBeNull();
  // The model picker is described by the alert, and the alert is under it.
  const alertId = alert.parentElement!.id;
  const select = [...container.querySelectorAll("[aria-describedby]")].find((node) => node.getAttribute("aria-describedby")?.split(" ").includes(alertId))!;
  expect(select.getAttribute("aria-describedby")?.split(" ")).toContain(alertId);
  expect(exposedStatuses()).toEqual([`${isolate("Codex")} could not list its models.`]);
  expect(exposedStatuses()[0]).not.toContain("error: models unavailable");
  // Opened last: an open popover hides what is behind it from the status regions above.
  const dialog = openDetails(alert);
  expect(dialog.querySelector("pre")?.textContent).toBe(detail);
  expect(description.contains(dialog)).toBe(false);
});


it("retains request-refusal metadata for rich model descriptions while keeping status announcements plain", async () => {
  const failure = new DaemonRequestError("Plain model request failure", "unknown_account", { account: "missing-account" });
  const { toggle, exposedStatuses, wait, container } = mount(failure);
  toggle();
  await wait(PAST_FIRST_WRITE);
  expect(container.querySelector("[data-slot=alert-description] code")?.textContent).toBe("missing-account");
  expect(exposedStatuses()).toEqual([`Unknown account ${isolate("missing-account")}.`]);
});

it("directs inherited model and effort badges without selecting or submitting their display values", () => {
  const inherited = { model: "model/中文-مراجعة", effort: "effort/مراجعة-中文" };
  const changed = vi.fn();
  const launch: LaunchModels = { loading: false, models: [], modelText: undefined, modelDescription: undefined, modelAlert: undefined, modelAlertId: undefined };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<>
    <ModelPicker launch={launch} model={null} inherit={inherited} onChange={changed} />
    <EffortPicker launch={launch} model={null} effort={null} inherit={inherited} onChange={changed} />
  </>));
  for (const value of Object.values(inherited)) {
    const badge = [...container.querySelectorAll("button span")].find((node) => node.textContent === value)!;
    expect(badge.getAttribute("dir")).toBe("auto");
    expect(badge.textContent).not.toMatch(/[\u2066-\u2069]/);
  }
  expect([...container.querySelectorAll("select")].map((select) => select.value)).toEqual(["", ""]);
  expect([...container.querySelectorAll("option")].map((option) => option.value)).not.toEqual(expect.arrayContaining(Object.values(inherited)));
  expect(changed).not.toHaveBeenCalled();
});
