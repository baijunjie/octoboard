// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import type { ShownStatusKind } from "../../sessions/sessionStatus";
import { AvatarStatusBadge, TopStatusMarker } from "./TopStatusMarker";
import { enter, leave, mountForTooltip, openPopover, openTooltip, press, trigger } from "../hint/tooltipTestSupport";
import { fakeTimers, runOutTimers } from "../../test/clock";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const STATUS_TEXT: Array<[Exclude<ShownStatusKind, "archived">, string]> = [
  ["waiting_user", "Waiting for you"],
  ["dormant", "Dormant"],
  ["paused", "Paused"],
  ["done", "Done"],
  ["idle", "Awaiting instructions"],
  ["delegated", "Child sessions working"],
  ["background", "Working in background"],
  ["working", "Working"],
  ["starting", "Starting"],
  ["switching", "Switching account"],
  ["reconnecting", "Reconnecting"],
  ["interrupted", "Interrupted"],
];

it.each(STATUS_TEXT)("says the %s status in words when the pointer rests on its marker", (status, text) => {
  fakeTimers();
  const container = mountForTooltip(<TopStatusMarker status={status} />);

  enter(trigger(container));
  expect(openTooltip()).toBeNull();
  runOutTimers();
  expect(openTooltip()?.textContent).toBe(text);

  leave(trigger(container));
  runOutTimers();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("draws every status its own glyph, starting, switching account and reconnecting sharing the turning arc", () => {
  const glyphs = STATUS_TEXT.map(([status]) => trigger(mountForTooltip(<TopStatusMarker status={status} />)).firstElementChild!.outerHTML);
  expect(new Set(glyphs).size).toBe(STATUS_TEXT.length - 2);
  expect(glyphs[STATUS_TEXT.findIndex(([status]) => status === "starting")]).toBe(glyphs[STATUS_TEXT.findIndex(([status]) => status === "switching")]);
  expect(glyphs[STATUS_TEXT.findIndex(([status]) => status === "starting")]).toBe(glyphs[STATUS_TEXT.findIndex(([status]) => status === "reconnecting")]);
});

it("adds nothing to the accessible name of the row or chip the marker sits in", () => {
  const container = mountForTooltip(<TopStatusMarker status="waiting_user" />);
  expect(trigger(container).hasAttribute("aria-label")).toBe(false);
  expect(container.querySelector("[role=img]")).toBeNull();
  expect(container.querySelector("[aria-hidden=true]")).not.toBeNull();
});

it("draws no marker, and so no tooltip, where no session is beneath it", () => {
  const container = mountForTooltip(<TopStatusMarker status={undefined} />);
  expect(container.firstElementChild).toBeNull();
});

it("opens no popover on a tap: both places it stands in are pressable, and the tap is theirs", () => {
  const container = mountForTooltip(<TopStatusMarker status="waiting_user" />);
  press(trigger(container));
  expect(openPopover()).toBeNull();
});

describe("AvatarStatusBadge", () => {
  const badge = (status: ShownStatusKind | undefined) => mountForTooltip(<AvatarStatusBadge status={status} />).firstElementChild;

  it.each(STATUS_TEXT.filter(([status]) => ["waiting_user", "paused", "done", "idle"].includes(status)))("gives a console showing %s a decorative badge", (status) => {
    const element = badge(status);
    expect(element).not.toBeNull();
    expect(element?.getAttribute("aria-hidden")).toBe("true");
  });

  it.each([undefined, "archived", "starting", "switching", "reconnecting", "dormant", "delegated", "background", "working", "interrupted"] as const)("draws none for %s", (status) => {
    expect(badge(status)).toBeNull();
  });

  it("gives each summary status a different glyph", () => {
    const glyphs = (["waiting_user", "paused", "done", "idle"] as const).map((status) => badge(status)!.firstElementChild!.outerHTML);
    expect(new Set(glyphs).size).toBe(4);
  });
});
