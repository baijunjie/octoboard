// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import type { ShownStatusKind } from "../../sessions/sessionStatus";
import { AvatarStatusBadge, TopStatusMarker } from "./TopStatusMarker";
import { enter, leave, mountForTooltip, openPopover, openTooltip, press, trigger, withFakeTimers } from "../hint/tooltipTestSupport";

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
  ["switching", "Switching account…"],
  ["interrupted", "Interrupted"],
];

it.each(STATUS_TEXT)("says the %s status in words when the pointer rests on its marker", (status, text) => {
  const settle = withFakeTimers();
  const container = mountForTooltip(<TopStatusMarker status={status} />);

  enter(trigger(container));
  expect(openTooltip()).toBeNull();
  settle();
  expect(openTooltip()?.textContent).toBe(text);

  leave(trigger(container));
  settle();
  expect(openTooltip()).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("draws every status its own glyph, starting and switching account sharing the turning arc", () => {
  const glyphs = STATUS_TEXT.map(([status]) => trigger(mountForTooltip(<TopStatusMarker status={status} />)).firstElementChild!.outerHTML);
  expect(new Set(glyphs).size).toBe(STATUS_TEXT.length - 1);
  expect(glyphs[STATUS_TEXT.findIndex(([status]) => status === "starting")]).toBe(glyphs[STATUS_TEXT.findIndex(([status]) => status === "switching")]);
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

  it.each(STATUS_TEXT)("gives a console showing %s a decorative badge", (status) => {
    const element = badge(status);
    expect(element).not.toBeNull();
    expect(element?.getAttribute("aria-hidden")).toBe("true");
  });

  it("draws none for an archived or absent status", () => {
    expect(badge("archived")).toBeNull();
    expect(badge(undefined)).toBeNull();
  });

  it("tells the statuses apart, starting and switching account sharing the grey turning arc", () => {
    // The glyph alone, so that a disc's colour is never what tells two statuses apart.
    const glyphs = STATUS_TEXT.map(([status]) => badge(status)!.firstElementChild!.outerHTML);
    expect(new Set(glyphs).size).toBe(STATUS_TEXT.length - 1);
    const grey = ["starting", "switching", "interrupted"].map((status) => badge(status as ShownStatusKind)!);
    for (const element of grey) expect(element.className).toContain("bg-default");
    expect(grey[0].outerHTML).toBe(grey[1].outerHTML);
    expect(grey[2].outerHTML).not.toBe(grey[0].outerHTML);
  });
});
