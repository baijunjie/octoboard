// @vitest-environment jsdom
import { act, useLayoutEffect } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { providerModelDisplay } from "../agents/providers";
import { press } from "../components/alerts/detailsTestSupport";
import { firstWriteDelayPassed } from "../components/announce/statusAnnouncerTestSupport";
import { CodedError } from "../errors/codedError";
import { isolate } from "../i18n/isolate";
import { DaemonProvider, type Daemon } from "../store";
import { testDaemon } from "../test/testDaemon";
import { TEST_PROVIDERS } from "../test/testProviders";
import { LaunchMatrix } from "./ConsoleLaunchFields";
import { EffortPicker, LaunchSettingsFields, ModelPicker, type LaunchModels } from "./LaunchSettingsFields";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** How long a wait on a condition may take before the test gives up on it, with room for a cold run to transform the
 * modules the component loads on first use. */
const UNTIL_SHOWN = { timeout: 15_000 };

function mount(unavailable?: import("../protocol").ModelsUnavailable | Error) {
  const request = vi.fn(async (body: { type: string }) => {
    if (body.type === "list_agent_models") {
      if (unavailable instanceof Error) throw unavailable;
      if (unavailable) return { type: "agent_models", id: null, models: [], unavailable };
      throw new Error("Fixture has no models");
    }
    return { type: "ack", id: null };
  });
  const daemon = testDaemon({ request: request as Daemon["request"], showNotice: vi.fn() });
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
  return { toggle, exposedStatuses, request, container };
}

// The list is asked for again on each opening and its refusal is shown again. The status region is mounted
// anew with each opening and writes after its delay, so the refusal is said once; the same answer writes
// nothing new.
it("says the model list's failure again each time the section is opened, once, after the region is there", async () => {
  const { toggle, exposedStatuses, request } = mount();
  toggle();
  expect(exposedStatuses()).toEqual([""]);
  await vi.waitFor(() => expect(exposedStatuses()).toEqual(["Something unexpected went wrong."]), UNTIL_SHOWN);
  // A thrown error is a failure the application has no words for: the region says the generic sentence,
  // and the error's own message stays in the mark's popover.
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
  await firstWriteDelayPassed();
  expect(exposedStatuses()).toEqual(["Something unexpected went wrong."]);
  expect(writes).toEqual(["Something unexpected went wrong."]);
  expect(request).toHaveBeenCalledTimes(2);
});

it("shows why there is no list as a warning mark after the Model field's label, its popover holding the sentence and the diagnostics, and announces only the sentence once", async () => {
  const detail = "error: models unavailable\n    /tmp/مراجعة";
  const { toggle, exposedStatuses, container } = mount({ code: "models_command_failed", params: { agent: "Codex", detail }, message: "fallback" });
  toggle();
  // The mark waits on a module loaded on first use, which a cold run transforms first: wait for it, not for a time.
  await vi.waitFor(() => expect(modelMark(container)).not.toBeNull(), UNTIL_SHOWN);
  await firstWriteDelayPassed();
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
  const failure = new CodedError("Plain model request failure", "unknown_account", { account: "missing-account" });
  const { toggle, exposedStatuses, container } = mount(failure);
  toggle();
  await vi.waitFor(() => expect(exposedStatuses()).toEqual([`Unknown account ${isolate("missing-account")}.`]), UNTIL_SHOWN);
  press(modelMark(container)!);
  expect(document.querySelector("[data-status-mark-popover] code")?.textContent).toBe("missing-account");
});

it("directs inherited model and effort badges without selecting or submitting their display values", () => {
  const inherited = { model: "model/中文-مراجعة", effort: "effort/مراجعة-中文" };
  const changed = vi.fn();
  const launch: LaunchModels = { agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel: null, defaultModelSource: null, loading: false, listed: true, models: [], modelText: undefined, modelFailure: undefined };
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

it("takes the effort levels from the default model while neither the pair nor its owner names a model", async () => {
  const models = [
    { id: "fable", name: "Fable", efforts: ["low", "max"] },
    { id: "opus", name: "Opus", efforts: ["medium"] },
  ];
  const launch: LaunchModels = { agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel: "fable", defaultModelSource: "best_available", loading: false, listed: true, models, modelText: undefined, modelFailure: undefined };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const levels = () => [...container.querySelectorAll("option")].map((option) => option.value).filter(Boolean);
  act(() => root.render(<EffortPicker launch={launch} model={null} effort={null} onChange={() => {}} />));
  expect(levels()).toEqual(["low", "max"]);
  // A model the pair names, or the one its owner names, comes before the default.
  act(() => root.render(<EffortPicker launch={launch} model="opus" effort={null} onChange={() => {}} />));
  expect(levels()).toEqual(["medium"]);
  act(() => root.render(<EffortPicker launch={launch} model={null} effort={null} inherit={{ model: "opus", effort: null }} onChange={() => {}} />));
  expect(levels()).toEqual(["medium"]);
});

it("says the agent's own setting, not Best available, for a default model Claude Code's configuration names, and names it as a session reads it", () => {
  const launchOf = (defaultModel: string, defaultModelSource: LaunchModels["defaultModelSource"]): LaunchModels =>
    ({ agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel, defaultModelSource, loading: false, listed: true, models: [{ id: "fable", name: "Fable", efforts: [], version: "Fable 5.1" }], modelText: undefined, modelFailure: undefined });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  const shown = (launch: LaunchModels, inherit?: { model: string | null; effort: string | null }) => {
    act(() => root.render(<ModelPicker launch={launch} model={null} inherit={inherit} onChange={() => {}} />));
    return container.querySelector("button")!.textContent;
  };
  expect(shown(launchOf("fable", "best_available"))).toBe("Best available, Fable 5.1");
  expect(shown(launchOf("fable", "configured"))).toBe("The agent's own setting, Fable 5.1");
  // An id the list lacks is read as a session's model is; one not recognised stays as it is.
  expect(shown(launchOf("claude-opus-4-5-20251101", "configured"))).toBe("The agent's own setting, Opus 4.5");
  expect(shown(launchOf("opus[1m]", "configured"))).toBe("The agent's own setting, Opus [1m]");
  expect(shown(launchOf("my-gateway-model", "configured"))).toBe("The agent's own setting, my-gateway-model");
  // A project keeps "Same as console" whichever the source.
  expect(shown(launchOf("fable", "configured"), { model: null, effort: null })).toBe("Same as console, Fable 5.1");
});

it("hands focus to the Model trigger when the mark that holds it goes, and leaves other focus alone", async () => {
  const launchOf = (modelFailure?: string): LaunchModels => ({ agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel: null, defaultModelSource: null, loading: false, listed: true, models: [], modelText: undefined, modelFailure });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
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
  const launchOf = (modelFailure?: string): LaunchModels => ({ agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel: null, defaultModelSource: null, loading: false, listed: true, models: [], modelText: undefined, modelFailure });
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

it("turns the focus ring on only when the console matrix's mark hands focus on, not when the whole matrix goes with it", async () => {
  const launchOf = (modelFailure?: string): LaunchModels => ({ agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel: null, defaultModelSource: null, loading: false, listed: true, models: [], modelText: undefined, modelFailure });
  const modality = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [modality]));
  });
  document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
  const pair = { model: null, effort: null };
  const render = (shown: boolean, modelFailure?: string) => act(async () => root.render(shown
    ? <LaunchMatrix launch={launchOf(modelFailure)} consoleSessions={pair} projectSessions={pair} onConsoleChange={() => {}} onProjectChange={() => {}} />
    : null));
  const focusMarkAfterPointer = () => act(() => {
    setInteractionModality("pointer");
    modelMark(container)!.focus();
  });
  // The mark goes on its own: focus moves to the console's Model trigger, with the ring.
  await render(true, "Why");
  focusMarkAfterPointer();
  await render(true, undefined);
  expect(document.activeElement).toBe(container.querySelectorAll("[data-slot=select-trigger]")[0]);
  expect(getInteractionModality()).toBe("keyboard");
  // The matrix goes with it, as when its dialog closes: nothing is moved, and the modality is left as it was.
  await render(true, "Why");
  focusMarkAfterPointer();
  await render(false);
  expect(document.activeElement).toBe(document.body);
  expect(getInteractionModality()).toBe("pointer");
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
  const daemon = testDaemon({ request: request as Daemon["request"], showNotice: vi.fn() });
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
  await act(async () => { await Promise.allSettled(request.mock.results.map((result) => result.value)); });
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

function MarkProbe({ container, marks }: { container: HTMLElement; marks: boolean[] }) {
  useLayoutEffect(() => {
    marks.push(modelMark(container) !== null);
  });
  return null;
}

// A refusal is not an answer: only what the daemon gave is kept for a pair, and a later refusal leaves it be.
// `replies` holds what each ask of the agent gets, in order; `hang` is an ask that never comes back.
type Reply = "list" | "refuse" | "hang";
function mountAsking(replies: Record<"claude" | "codex", Reply[]>) {
  const asked = { claude: 0, codex: 0 };
  const unexpected: string[] = [];
  // The asks that come back, one way or the other, for `settle` to wait on.
  const answered: Promise<unknown>[] = [];
  const request = vi.fn((body: { type: string; agent?: "claude" | "codex" }): Promise<unknown> => {
    if (body.type !== "list_agent_models") return Promise.resolve({ type: "ack", id: null });
    const reply = replies[body.agent!][asked[body.agent!]++];
    if (reply === "hang") return new Promise<never>(() => {});
    let answer: Promise<unknown>;
    if (reply === undefined) {
      unexpected.push(body.agent!);
      answer = Promise.reject(new Error(`Unexpected ask for ${body.agent}`));
    } else if (reply === "refuse") {
      answer = Promise.reject(new Error("Refused"));
    } else {
      answer = Promise.resolve({ type: "agent_models", id: null, agent: body.agent, account: null, models: [{ id: `${body.agent}-model`, name: `${body.agent} model`, efforts: [] }], unavailable: null });
    }
    answered.push(answer);
    return answer;
  });
  const daemon = testDaemon({ request: request as Daemon["request"], showNotice: vi.fn() });
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    expect(unexpected).toEqual([]);
  });
  // Whether the failure mark is in the DOM at each commit `render` makes, read in a layout effect: before the passive
  // effects of that commit, which is where the first paint after a reopening is decided.
  const marks: boolean[] = [];
  const render = (agent: "claude" | "codex", expanded = true) => act(() =>
    root.render(
      <DaemonProvider value={daemon}>
        <LaunchSettingsFields agent={agent} account={null} model={null} effort={null} onModelChange={() => {}} onEffortChange={() => {}} expanded={expanded} onExpandedChange={() => {}} />
        <MarkProbe container={container} marks={marks} />
      </DaemonProvider>,
    ),
  );
  const settle = () => act(async () => { await Promise.allSettled(answered); });
  const trigger = () => container.querySelector<HTMLButtonElement>("[data-slot=select-trigger]")!;
  const options = () => [...container.querySelectorAll("option")].map((option) => option.value);
  return { render, settle, trigger, options, container, request, marks };
}

// A form returning to a loaded agent keeps its last choices available while refreshing them.
it("shows the list an agent was last given while asking again on going back to it, and locks the pickers only on the first ask", async () => {
  const { render, settle, trigger, options, request } = mountAsking({ claude: ["list", "hang"], codex: ["list"] });
  const loadingPlaceholder = "Loading…";
  render("claude");
  expect(trigger().disabled).toBe(true);
  await settle();
  expect(trigger().disabled).toBe(false);
  render("codex");
  expect(trigger().disabled).toBe(true);
  expect(trigger().textContent).toBe(loadingPlaceholder);
  await settle();

  render("claude");
  await settle();
  expect(trigger().disabled).toBe(false);
  expect(trigger().textContent).not.toBe(loadingPlaceholder);
  expect(options()).toContain("claude-model");
  expect(request.mock.calls.filter(([body]) => body.agent === "claude")).toHaveLength(2);
});

it.each([
  ["going away and back", false],
  ["collapsing and opening the section", true],
] as const)("locks the pickers and shows the loading placeholder again when a pair that has had only refusals is asked again, after %s", async (_, collapse) => {
  const { render, settle, trigger, container, marks } = mountAsking({ claude: ["refuse", "hang"], codex: ["list"] });
  render("claude");
  await settle();
  expect(modelMark(container)).not.toBeNull();
  if (collapse) render("claude", false);
  else render("codex");
  await settle();
  const before = marks.length;
  render("claude");
  // Not even the first commit back shows the old refusal, before the ask is sent.
  expect(marks.slice(before)).not.toHaveLength(0);
  expect(marks.slice(before)).not.toContain(true);
  expect(trigger().disabled).toBe(true);
  expect(trigger().textContent).toBe("Loading…");
});

it.each([
  ["going away and back", false],
  ["collapsing and opening the section", true],
] as const)("keeps the list a pair was given when a later ask is refused, after %s", async (_, collapse) => {
  const { render, settle, trigger, options, container } = mountAsking({ claude: ["list", "refuse", "refuse"], codex: ["list", "list"] });
  render("claude");
  await settle();
  const away = () => (collapse ? render("claude", false) : render("codex"));
  away();
  await settle();
  render("claude");
  await settle();
  expect(options()).toContain("claude-model");
  expect(trigger().disabled).toBe(false);
  // Away and back once more: the refusal the hook now holds must not show in place of the list.
  away();
  await settle();
  render("claude");
  expect(options()).toContain("claude-model");
  await settle();
  expect(options()).toContain("claude-model");
  expect(modelMark(container)).toBeNull();
});

it("keeps the mark clear of the Model field's own press: pressing it neither opens the list nor clears the value", () => {
  const launch: LaunchModels = { agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel: null, defaultModelSource: null, loading: false, listed: true, models: [{ id: "alpha", name: "alpha" }] as LaunchModels["models"], modelText: undefined, modelFailure: "Why" };
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


it("shows loading inside all four console launch selectors without a note below the model selectors", () => {
  const launch: LaunchModels = { agent: "claude", display: providerModelDisplay(TEST_PROVIDERS, "claude"), defaultModel: null, defaultModelSource: null, loading: true, listed: false, models: [], modelText: "Loading the models this agent offers…", modelFailure: undefined };
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => { act(() => root.unmount()); container.remove(); });
  act(() => root.render(<LaunchMatrix launch={launch} consoleSessions={{ model: null, effort: null }} projectSessions={{ model: null, effort: null }} onConsoleChange={() => {}} onProjectChange={() => {}} />));
  const triggers = [...container.querySelectorAll<HTMLButtonElement>("[data-slot=select-trigger]")];
  expect(triggers).toHaveLength(4);
  for (const trigger of triggers) {
    expect(trigger.textContent).toBe("Loading…");
    expect(trigger.disabled).toBe(true);
    expect(trigger.getAttribute("aria-busy")).toBe("true");
    expect(trigger.querySelector("[data-slot=spinner]")).not.toBeNull();
  }
  for (const field of container.querySelectorAll("[data-slot=select]")) expect(field.querySelector("[data-slot=description]")).toBeNull();
});
