// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, onTestFinished, vi } from "vitest";

import { press } from "../components/detailsTestSupport";
import { DaemonRequestError } from "../daemon-client";
import { isolate } from "../i18n/isolate";
import { createStateStore, DaemonProvider, type Daemon } from "../store";
import { LaunchMatrix } from "./ConsoleLaunchFields";
import { EffortPicker, LaunchSettingsFields, ModelPicker, type LaunchModels } from "./LaunchSettingsFields";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Longer than `StatusAnnouncer`'s first write delay. */
const PAST_FIRST_WRITE = 150;

function mount(unavailable?: import("../protocol").ModelsUnavailable | Error) {
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type === "list_agent_models") {
      if (unavailable instanceof Error) throw unavailable;
      if (unavailable) return { type: "agent_models", id: null, models: [], unavailable };
      throw new Error("Fixture has no models");
    }
    return { type: "ack", id: null };
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
  // A thrown error is a failure the application has no words for: the region says the generic sentence,
  // and the error's own message stays in the mark's popover.
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

it("shows why there is no list as a warning mark after the Model field's label, its popover holding the sentence and the diagnostics, and announces only the sentence once", async () => {
  const detail = "error: models unavailable\n    /tmp/مراجعة";
  const { toggle, exposedStatuses, wait, container } = mount({ code: "models_command_failed", params: { agent: "Codex", detail }, message: "fallback" });
  toggle();
  await wait(PAST_FIRST_WRITE);
  // No Alert under the field, and no loose text: the failure is the mark and its popover.
  expect(container.querySelector('[data-slot="alert-root"]')).toBeNull();
  expect(container.querySelector("pre")).toBeNull();
  expect(container.querySelector("[data-slot=select]")!.textContent).not.toContain("could not list its models");
  // The mark is about the whole field: a warning, right after the label, beside it rather than in it, and not
  // in the trigger or over its room, so the field is still named "Model" alone.
  const mark = modelMark(container)!;
  const field = container.querySelector<HTMLElement>("[data-slot=select]")!;
  const trigger = field.querySelector<HTMLElement>("[data-slot=select-trigger]")!;
  const label = field.querySelector<HTMLElement>("[data-slot=label]")!;
  expect(mark.querySelector("svg")!.getAttribute("class")).toContain("text-warning-soft-foreground");
  expect(field.contains(mark)).toBe(true);
  expect(trigger.contains(mark)).toBe(false);
  expect(label.contains(mark)).toBe(false);
  expect(label.compareDocumentPosition(mark) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(mark.compareDocumentPosition(trigger) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(label.textContent).toBe("Model");
  // The trigger keeps only the clear button's room, none for a mark.
  expect(trigger.querySelectorAll(":scope > span[aria-hidden='true'].w-6")).toHaveLength(1);
  // Nothing describes the field by an element that is not there.
  const described = (field.querySelector("button[aria-describedby]") ?? trigger).getAttribute("aria-describedby");
  for (const id of (described ?? "").split(" ").filter(Boolean)) expect(document.getElementById(id)).not.toBeNull();
  expect(exposedStatuses()).toEqual([`${isolate("Codex")} could not list its models.`]);
  expect(exposedStatuses()[0]).not.toContain("error: models unavailable");
  // Opened last: an open popover hides what is behind it from the status regions above.
  press(mark);
  const popover = document.querySelector("[data-status-mark-popover]")!;
  expect(popover.textContent).toContain("could not list its models.");
  expect(popover.querySelector("pre")?.textContent).toBe(detail);
  // The failure is not retried from the mark: it has no Try again.
  expect([...popover.querySelectorAll("button")].filter((b) => b.textContent === "Try again")).toHaveLength(0);
});

/** The mark after the Model field's label, if the field has one. */
const modelMark = (container: HTMLElement) => container.querySelector<HTMLElement>("button[data-status-mark][aria-label='Could not load the models']");

it("retains request-refusal metadata for rich model descriptions in the mark's popover while keeping status announcements plain", async () => {
  const failure = new DaemonRequestError("Plain model request failure", "unknown_account", { account: "missing-account" });
  const { toggle, exposedStatuses, wait, container } = mount(failure);
  toggle();
  await wait(PAST_FIRST_WRITE);
  expect(exposedStatuses()).toEqual([`Unknown account ${isolate("missing-account")}.`]);
  press(modelMark(container)!);
  expect(document.querySelector("[data-status-mark-popover] code")?.textContent).toBe("missing-account");
});

it("directs inherited model and effort badges without selecting or submitting their display values", () => {
  const inherited = { model: "model/中文-مراجعة", effort: "effort/مراجعة-中文" };
  const changed = vi.fn();
  const launch: LaunchModels = { loading: false, listed: true, models: [], modelText: undefined, modelDescription: undefined, modelFailure: undefined };
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

it("hands focus to the Model trigger when the mark that holds it goes, and leaves other focus alone", async () => {
  const launchOf = (modelFailure?: string): LaunchModels => ({ loading: false, listed: true, models: [], modelText: undefined, modelDescription: undefined, modelFailure });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  // The hand-off decides after the commit, in a microtask, which `act` waits for only when awaited.
  const render = (modelFailure?: string) => act(async () => root.render(<ModelPicker launch={launchOf(modelFailure)} model={null} onChange={() => {}} />));
  await render("Why");
  const trigger = container.querySelector<HTMLElement>("[data-slot=select-trigger]")!;
  act(() => modelMark(container)!.focus());
  expect(document.activeElement).toBe(modelMark(container));
  await render(undefined);
  expect(modelMark(container)).toBeNull();
  expect(document.activeElement).toBe(trigger);
  // Focus elsewhere stays there.
  await render("Why");
  const outside = document.body.appendChild(document.createElement("button"));
  onTestFinished(() => outside.remove());
  act(() => outside.focus());
  await render(undefined);
  expect(document.activeElement).toBe(outside);
});

it("hands focus to the console's Model trigger when the console matrix's mark that holds it goes", async () => {
  const launchOf = (modelFailure?: string): LaunchModels => ({ loading: false, listed: true, models: [], modelText: undefined, modelDescription: undefined, modelFailure });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const pair = { model: null, effort: null };
  const render = (modelFailure?: string) => act(async () => root.render(<LaunchMatrix launch={launchOf(modelFailure)} consoleSessions={pair} projectSessions={pair} onConsoleChange={() => {}} onProjectChange={() => {}} />));
  await render("Why");
  const marks = container.querySelectorAll<HTMLElement>("button[data-status-mark][aria-label='Could not load the models']");
  expect(marks).toHaveLength(3);
  const consoleTrigger = container.querySelectorAll<HTMLElement>("[data-slot=select-trigger]")[0]!;
  // The stacked layout's mark above the project's picker, the last of the three places.
  act(() => marks[2]!.focus());
  await render(undefined);
  expect(modelMark(container)).toBeNull();
  expect(document.activeElement).toBe(consoleTrigger);
});

// WebKit blurs a control as it is disabled, before the render's effects run; jsdom never does, so the blur is
// made by hand.
it.each([
  ["the section's trigger", false],
  ["the selected tab, in a section with tabs,", true],
] as const)("hands focus on to %s when the mark that holds it goes in the render that locks the Model trigger", async (_, withTabs) => {
  let reload = false;
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type !== "list_agent_models") return { type: "ack", id: null };
    if (reload) return new Promise<never>(() => {});
    return { type: "agent_models", id: null, models: [], unavailable: { code: "agent-not-found", message: "Gone", params: {} } };
  });
  const daemon = { store: createStateStore({}), request, toastError: vi.fn(), toastNotice: vi.fn() } as unknown as Daemon;
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const render = (account: string | null) => act(async () =>
    root.render(
      <DaemonProvider value={daemon}>
        <LaunchSettingsFields
          agent="claude"
          account={account}
          model={null}
          effort={null}
          onModelChange={() => {}}
          onEffortChange={() => {}}
          expanded
          onExpandedChange={() => {}}
          otherTab={withTabs ? { id: "other", label: "Other", content: null } : undefined}
        />
      </DaemonProvider>,
    ),
  );
  await render(null);
  await act(() => new Promise((resolve) => setTimeout(resolve, 0)));
  const trigger = container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  const blurWhenDisabled = (name: string) => {
    if (name === "disabled" && document.activeElement === trigger) trigger.blur();
  };
  const setAttribute = trigger.setAttribute.bind(trigger);
  trigger.setAttribute = (name: string, value: string) => {
    blurWhenDisabled(name);
    setAttribute(name, value);
  };
  const disabled = Object.getOwnPropertyDescriptor(HTMLButtonElement.prototype, "disabled")!;
  Object.defineProperty(trigger, "disabled", {
    configurable: true,
    get: () => disabled.get!.call(trigger),
    set: (value: boolean) => {
      if (value) blurWhenDisabled("disabled");
      disabled.set!.call(trigger, value);
    },
  });
  act(() => modelMark(container)!.focus());
  expect(document.activeElement).toBe(modelMark(container));
  reload = true;
  await render("other");
  expect(modelMark(container)).toBeNull();
  expect(trigger.disabled).toBe(true);
  const disclosure = [...container.querySelectorAll("button")].find((button) => button.textContent === "Advanced")!;
  const launchTab = [...container.querySelectorAll('[role="tab"]')].find((tab) => tab.textContent === "Launch settings");
  expect(document.activeElement).toBe(withTabs ? launchTab : disclosure);
});

it("keeps the mark clear of the Model field's own press: pressing it neither opens the list nor clears the value", () => {
  const launch: LaunchModels = { loading: false, listed: true, models: [{ id: "alpha", name: "alpha" }] as LaunchModels["models"], modelText: undefined, modelDescription: undefined, modelFailure: "Why" };
  const changed = vi.fn();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<ModelPicker launch={launch} model="alpha" onChange={changed} />));
  press(modelMark(container)!);
  expect(document.querySelector("[data-status-mark-popover]")).not.toBeNull();
  expect(container.querySelector("[data-slot=select-trigger]")!.getAttribute("aria-expanded")).toBe("false");
  expect(changed).not.toHaveBeenCalled();
});
