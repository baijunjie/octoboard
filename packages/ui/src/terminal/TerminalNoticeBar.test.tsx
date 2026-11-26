// @vitest-environment jsdom
import { act } from "react";
import { expect, it, vi } from "vitest";

import { Button } from "../primitives";
import { mountHost } from "../test/mountHost";
import { TerminalNoticeBar, TerminalNoticeSlot } from "./TerminalNoticeBar";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const warningMark = (root: ParentNode) => root.querySelector("svg[class*=triangle-alert]");

it("hides its sentence from assistive technology and leaves the one button reachable", () => {
  const onPress = vi.fn();
  const container = mountHost(<TerminalNoticeBar sentence="The process ended." action="Restart" variant="primary" onPress={onPress} />);
  expect(container.querySelector("span[aria-hidden=true]")!.textContent).toBe("The process ended.");
  const button = [...container.querySelectorAll("button")].find((candidate) => candidate.textContent === "Restart")!;
  expect(button.closest("[aria-hidden=true]")).toBeNull();
  act(() => button.click());
  expect(onPress).toHaveBeenCalledTimes(1);
});

it("leads with the warning mark in its warning kind only, hidden from assistive technology, so the kind is not told by colour alone", () => {
  const plain = mountHost(<TerminalNoticeBar sentence="Ended." action="Restart" variant="secondary" onPress={() => {}} />);
  expect(warningMark(plain)).toBeNull();
  const warning = mountHost(<TerminalNoticeBar kind="warning" sentence="Runs without hooks." action="Details" variant="secondary" onPress={() => {}} />);
  expect(warningMark(warning)?.getAttribute("aria-hidden")).toBe("true");
});

it("ends with the controls it is given in place of one button", () => {
  const container = mountHost(
    <TerminalNoticeBar
      kind="warning"
      compact
      sentence="Runs without hooks."
      controls={<><Button size="xs" variant="secondary">Details</Button><Button size="xs" variant="ghost" label="Dismiss notice">x</Button></>}
    />,
  );
  const names = [...container.querySelectorAll("button")].map((button) => button.getAttribute("aria-label") ?? button.textContent);
  expect(names).toEqual(["Details", "Dismiss notice"]);
  expect(container.querySelector("span[aria-hidden=true]")!.textContent).toBe("Runs without hooks.");
});

it("stacks the bars that share a slot, centred, at the edge it is given", () => {
  const bottom = mountHost(<TerminalNoticeSlot><i /></TerminalNoticeSlot>);
  const slot = bottom.firstElementChild as HTMLElement;
  expect(slot.className).toContain("flex-col");
  expect(slot.className).toContain("items-center");
  expect(slot.className).toContain("bottom-4");
  const top = mountHost(<TerminalNoticeSlot edge="top"><i /></TerminalNoticeSlot>);
  expect((top.firstElementChild as HTMLElement).className).toContain("top-4");
});
