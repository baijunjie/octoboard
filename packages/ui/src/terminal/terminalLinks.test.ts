// @vitest-environment jsdom
import { expect, it, vi } from "vitest";

import { IS_MAC } from "../useWindowShortcut";
import { linkBox, TERMINAL_URL_REGEX, terminalLinkHandler } from "./terminalLinks";

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

it("ends a plain-text address at the first non-ASCII character", () => {
  const match = (text: string) => TERMINAL_URL_REGEX.exec(text)?.[0];
  expect(match("看 http://localhost:5301/a.html?s=x（加&t=d")).toBe("http://localhost:5301/a.html?s=x");
  expect(match("http://localhost:5301/a.html看深色")).toBe("http://localhost:5301/a.html");
});
