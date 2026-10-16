// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";

import { type ConsoleSwitchFollowUp, useConsoleSwitchFollowUp } from "./useConsoleSwitchFollowUp";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

afterEach(() => {
  act(() => root?.unmount());
  document.body.replaceChildren();
  root = undefined;
});

/** Mounts the hook on console `c-a`; `show` makes another console current. */
function mount() {
  const apply = vi.fn();
  const restore = vi.fn();
  let latest!: ConsoleSwitchFollowUp;
  function Probe({ consoleId }: { consoleId: string }) {
    latest = useConsoleSwitchFollowUp(consoleId, apply, restore);
    return null;
  }
  root = createRoot(document.body.appendChild(document.createElement("div")));
  const show = (consoleId: string) => act(() => root!.render(<Probe consoleId={consoleId} />));
  show("c-a");
  apply.mockClear();
  return { apply, restore, show, followUp: () => latest };
}

it("applies the rule at once when another console is made current without a press", () => {
  const { apply, show } = mount();
  show("c-b");
  expect(apply).toHaveBeenCalledTimes(1);
});

it.each([
  ["selects a row", ["selection", "owner"], undefined],
  ["only sets the owner", ["owner"], { selection: true, owner: false }],
  ["selects nothing", [], { selection: true, owner: true }],
] as const)("restores only what the press that %s did not choose", (_name, claims, expected) => {
  const { apply, restore, show, followUp } = mount();
  act(() => followUp().holdForPress());
  show("c-b");
  expect(restore).not.toHaveBeenCalled();
  act(() => claims.forEach((what) => followUp().claim(what)));
  act(() => followUp().releasePress());
  if (expected) expect(restore).toHaveBeenCalledExactlyOnceWith(expected);
  else expect(restore).not.toHaveBeenCalled();
  expect(apply).not.toHaveBeenCalled();
  show("c-a");
  expect(apply).toHaveBeenCalledTimes(1);
});

it("does nothing at the end of a press that changed no console", () => {
  const { apply, restore, followUp } = mount();
  act(() => followUp().holdForPress());
  act(() => followUp().releasePress());
  expect(apply).not.toHaveBeenCalled();
  expect(restore).not.toHaveBeenCalled();
});
