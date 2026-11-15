// @vitest-environment jsdom
import { act, StrictMode, useLayoutEffect } from "react";
import { createRoot } from "react-dom/client";
import { setInteractionModality } from "react-aria";
import { Button } from "react-aria-components";
import { getInteractionModality } from "react-aria/private/interactions/useFocusVisible";
import { expect, it, onTestFinished, vi } from "vitest";

import { HandoffOnUnmount, useHandoffAfterRemoval } from "./HandoffOnUnmount";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

/** Sends the last input before a handoff: a press of a pointer, or a hardware keyboard's navigation key. */
function lastInput(input: "key" | "mouse" | "touch") {
  if (input === "key") document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true }));
  else document.dispatchEvent(new PointerEvent("pointerdown", { pointerType: input, bubbles: true }));
}

/** A pane holding focus that goes away, handing focus to a control outside it. */
function handoff(input: "key" | "mouse" | "touch") {
  lastInput(input);
  const modality = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [modality]));
  });
  const render = (shown: boolean) => act(() => root.render(
    <>
      <button type="button" id="target">target</button>
      {shown && (
        <>
          <HandoffOnUnmount from="#pane" to={() => document.getElementById("target")!.focus()} />
          <div id="pane"><button type="button">inside</button></div>
        </>
      )}
    </>,
  ));
  render(true);
  act(() => setInteractionModality("pointer"));
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  render(false);
  return document.activeElement?.id;
}

it("hands focus on with the keyboard's focus ring after a key", () => {
  expect(handoff("key")).toBe("target");
  expect(getInteractionModality()).toBe("keyboard");
});

it.each(["mouse", "touch"] as const)("hands focus on without a focus ring after a %s press", (input) => {
  expect(handoff(input)).toBe("target");
  expect(getInteractionModality()).toBe("pointer");
});

/** A component rendering a pane that holds focus, which hands it to a react-aria button beside it as it goes. */
function HandoffHost({ shown, disabled, restore }: { shown: boolean; disabled: boolean; restore: boolean }) {
  const handoffFrom = useHandoffAfterRemoval(() => document.getElementById("target"));
  return (
    <>
      <Button id="target" isDisabled={disabled}>target</Button>
      <button type="button" id="restored">restored</button>
      {shown ? (
        <>
          {handoffFrom("#pane")}
          <div id="pane"><button type="button">inside</button></div>
        </>
      ) : restore && <Restore />}
    </>
  );
}

/** Focus placed elsewhere in the same commit, by a layout effect of a descendant of the handoff's host. */
function Restore() {
  useLayoutEffect(() => document.getElementById("restored")!.focus(), []);
  return null;
}

it.each([
  ["moves focus, with the ring, when focus fell to <body> after a key", { mounted: true }, "target", "keyboard", "key"],
  ["moves focus, without a ring, when focus fell to <body> after a mouse press", { mounted: true }, "target", "pointer", "mouse"],
  ["moves nothing, and leaves the modality, when its host goes with the source", { mounted: false }, undefined, "pointer", "mouse"],
  ["leaves focus placed meanwhile, and the modality, alone", { mounted: true, restore: true }, "restored", "pointer", "mouse"],
  ["leaves a target disabled by the same render alone", { mounted: true, disabled: true }, undefined, "pointer", "mouse"],
  ["moves focus, without a ring, when focus fell to <body> from inside the source after a mouse press", { mounted: true, steps: ["fall"] }, "target", "pointer", "mouse"],
  ["leaves focus alone, and the modality, when focus left the source for another element and then fell to <body>", { mounted: true, steps: ["restored", "fall"] }, undefined, "pointer", "mouse"],
] as const)("the handoff after the removal %s", (_, after: { mounted: boolean; restore?: boolean; disabled?: boolean; steps?: readonly ("restored" | "fall")[] }, focused, modality, input) => {
  lastInput(input);
  const before = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [before]));
  });
  act(() => root.render(<HandoffHost shown disabled={false} restore={false} />));
  act(() => setInteractionModality("pointer"));
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  for (const step of after.steps ?? []) {
    act(() => {
      if (step === "fall") (document.activeElement as HTMLElement).blur();
      else document.getElementById("restored")!.focus();
    });
  }
  act(() => root.render(after.mounted ? <HandoffHost shown={false} disabled={after.disabled ?? false} restore={after.restore ?? false} /> : null));
  expect(document.activeElement).toBe(focused ? document.getElementById(focused) : document.body);
  expect(getInteractionModality()).toBe(modality);
  if (focused === "target") expect(document.getElementById("target")!.hasAttribute("data-focus-visible")).toBe(modality === "keyboard");
});

it("lets a react-aria control taking focus in the commit show its focus ring after a key", async () => {
  lastInput("key");
  const before = getInteractionModality();
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
    act(() => Reflect.apply(setInteractionModality, undefined, [before]));
  });
  const render = (shown: boolean) => act(() => root.render(
    <>
      <Button id="target">target</Button>
      {shown && (
        <>
          <HandoffOnUnmount from="#pane" to={() => document.getElementById("target")!.focus()} />
          <div id="pane"><button type="button">inside</button></div>
        </>
      )}
    </>,
  ));
  render(true);
  act(() => setInteractionModality("pointer"));
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  render(false);
  const target = document.getElementById("target")!;
  expect(document.activeElement).toBe(target);
  await act(async () => {});
  expect(target.hasAttribute("data-focused")).toBe(true);
  expect(target.hasAttribute("data-focus-visible")).toBe(true);
});

type Step = "inside" | "outside" | "fall" | "pressInside" | "pressOutside" | "pressDocument";
type Form = "selector" | "function" | "opted-out";

/** A source (`#pane`) beside an outside button, with a `HandoffOnUnmount` for it taking `to`, whose focus the test
 * moves through `steps` before the whole source goes. A press is a `pointerdown` that moves no focus: `pressOutside` on
 * the page background, `pressDocument` aimed at the document itself, which is no element. `handoffLate` mounts the
 * handoff only after the first step, so focus is already inside when it mounts. The `function` form finds the pane
 * through a variable kept while mounted; `opted-out` is that function answering null once the source goes, as a caller
 * declining the handoff does. */
function removeSource(form: Form, steps: Step[], to: () => void, handoffLate = false) {
  const { container, root } = mountRoot();
  let pane: Element | null = null;
  let optedOut = false;
  const from = form === "selector" ? "#pane" : () => (optedOut ? null : pane);
  const render = (handoff: boolean, source: boolean) => act(() => root.render(
    <>
      <button type="button" id="outside">outside</button>
      {handoff && <HandoffOnUnmount key="handoff" from={from} to={to} />}
      {source && (
        <div key="pane" id="pane" ref={(element) => { if (element) pane = element; }}>
          <button type="button" id="inside">inside</button>
        </div>
      )}
    </>,
  ));
  const move = (step: Step) => act(() => {
    if (step === "fall") (document.activeElement as HTMLElement).blur();
    else if (step === "pressInside" || step === "pressOutside" || step === "pressDocument") {
      const target = step === "pressInside" ? container.querySelector("#inside")! : step === "pressOutside" ? document.body : document;
      target.dispatchEvent(new PointerEvent("pointerdown", { pointerType: "mouse", bubbles: true }));
    } else container.querySelector<HTMLElement>(`#${step}`)!.focus();
  });
  render(!handoffLate, true);
  if (handoffLate) {
    move(steps[0]!);
    render(true, true);
    steps.slice(1).forEach(move);
  } else steps.forEach(move);
  optedOut = form === "opted-out";
  render(false, false);
}

/** A root in the document that is unmounted, and removed, when the test ends. */
function mountRoot() {
  const container = document.body.appendChild(document.createElement("div"));
  const root = createRoot(container);
  onTestFinished(() => {
    act(() => root.unmount());
    container.remove();
  });
  return { container, root };
}

const FORMS = ["selector", "function"] as const;

it.each(FORMS)("hands focus on when focus fell to the page from inside the source (%s)", (form) => {
  const to = vi.fn();
  removeSource(form, ["inside", "fall"], to);
  expect(to).toHaveBeenCalledTimes(1);
});

it.each(FORMS)("hands focus on when focus was inside the source as the handoff mounted and then fell to the page (%s)", (form) => {
  const to = vi.fn();
  removeSource(form, ["inside", "fall"], to, true);
  expect(to).toHaveBeenCalledTimes(1);
});

it.each(FORMS)("hands focus on when focus came back inside the source before it fell to the page (%s)", (form) => {
  const to = vi.fn();
  removeSource(form, ["inside", "outside", "inside", "fall"], to);
  expect(to).toHaveBeenCalledTimes(1);
});

it.each(FORMS)("hands focus on when a press inside the source came before focus fell to the page (%s)", (form) => {
  const to = vi.fn();
  removeSource(form, ["inside", "pressInside", "fall"], to);
  expect(to).toHaveBeenCalledTimes(1);
});

const HANDS_NOTHING_ON: readonly (readonly [string, readonly Step[], boolean?])[] = [
  ["focus landed outside the source after leaving it and then fell to the page", ["inside", "outside", "fall"]],
  ["focus is on an element outside the source", ["inside", "outside"]],
  ["focus was never inside the source and fell to the page", ["outside", "fall"]],
  ["focus was outside the source as the handoff mounted and fell to the page", ["outside", "fall"], true],
  ["a press outside the source, which moved no focus, came before focus fell to the page", ["inside", "pressOutside", "fall"]],
  ["a press aimed at the document itself came before focus fell to the page", ["inside", "pressDocument", "fall"]],
];

it.each(HANDS_NOTHING_ON.flatMap(([name, steps, late]) => FORMS.map((form) => [name, form, steps, late] as const)))(
  "hands nothing on when %s (%s)",
  (_, form, steps, handoffLate) => {
    const to = vi.fn();
    removeSource(form, [...steps], to, handoffLate);
    expect(to).not.toHaveBeenCalled();
  },
);

it("hands nothing on when focus fell from inside the source and the function finding it answers null as the source goes", () => {
  const to = vi.fn();
  removeSource("opted-out", ["inside", "fall"], to);
  expect(to).not.toHaveBeenCalled();
});

it.each([
  ["", false],
  [" under StrictMode, whose trial unmount runs the cleanup once", true],
])("hands nothing on when the selector changes after focus fell from inside the source%s", (_, strict) => {
  const to = vi.fn();
  const { container, root } = mountRoot();
  const Wrapper = strict ? StrictMode : "div";
  const render = (selector: string) => act(() => root.render(
    <Wrapper>
      <HandoffOnUnmount from={selector} to={to} />
      <div id="pane"><button type="button">inside</button></div>
    </Wrapper>,
  ));
  render("#pane");
  act(() => container.querySelector<HTMLElement>("#pane button")!.focus());
  act(() => (document.activeElement as HTMLElement).blur());
  render("#other");
  expect(to).not.toHaveBeenCalled();
});
