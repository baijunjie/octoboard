// @vitest-environment jsdom
import { act } from "react";
import { expect, it } from "vitest";

import { mountHost } from "../../test/mountHost";
import { useOptimisticOrder } from "./useOptimisticOrder";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function mount(first: { base: string; settled: number[] }) {
  const held: { current?: ReturnType<typeof useOptimisticOrder<number[]>> } = {};
  function Harness({ base, settled }: { base: string; settled: number[] }) {
    held.current = useOptimisticOrder(base, settled);
    return null;
  }
  const host = mountHost(<Harness {...first} />);
  const render = (props: { base: string; settled: number[] }) => host.render(<Harness {...props} />);
  return { held, render };
}

it("shows a proposed order until its base changes, and not again when the base returns", () => {
  const { held, render } = mount({ base: "a", settled: [1, 2] });
  act(() => void held.current!.propose([2, 1]));
  expect(held.current!.order).toEqual([2, 1]);
  render({ base: "b", settled: [2, 1] });
  expect(held.current!.order).toEqual([2, 1]);
  render({ base: "a", settled: [1, 2] });
  expect(held.current!.order).toEqual([1, 2]);
});

it("takes an order back, unless a later proposal has replaced it", () => {
  const { held } = mount({ base: "a", settled: [1, 2, 3] });
  let first = () => {};
  act(() => { first = held.current!.propose([2, 1, 3]); });
  act(() => void held.current!.propose([3, 2, 1]));
  act(() => first());
  expect(held.current!.order).toEqual([3, 2, 1]);
  let second = () => {};
  act(() => { second = held.current!.propose([1, 3, 2]); });
  act(() => second());
  expect(held.current!.order).toEqual([1, 2, 3]);
});
