// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { IS_MAC } from "../useWindowShortcut";
import { linkBox, terminalLinkHandler } from "./terminalLinks";

it("opens a link only on the platform's shortcut modifier", () => {
  const open = vi.fn(async () => {});
  const onClick = terminalLinkHandler(open);
  onClick(new MouseEvent("click"), "https://example.com");
  expect(open).not.toHaveBeenCalled();
  onClick(new MouseEvent("click", IS_MAC ? { metaKey: true } : { ctrlKey: true }), "https://example.com");
  expect(open).toHaveBeenCalledWith("https://example.com");
});

it("boxes a link from the cell grid", () => {
  const grid = { cols: 10, rows: 5, viewportY: 20, screen: { left: 100, top: 50, width: 100, height: 100 } };
  expect(linkBox("u", { start: { x: 3, y: 22 }, end: { x: 5, y: 22 } }, grid)).toEqual({
    uri: "u", left: 120, top: 70, width: 30, height: 20,
  });
});
