import type { Terminal } from "@xterm/xterm";
import { expect, it } from "vitest";

import { hasLinkUnderPointer } from "./xtermInternals";

const termWith = (core: unknown) => ({ _core: core }) as unknown as Terminal;

it("finds a link under the pointer where xterm's link detection holds one", () => {
  expect(hasLinkUnderPointer(termWith({ linkifier: { currentLink: { link: {} } } }))).toBe(true);
});

it.each([
  ["no link detection holds a link", { linkifier: { currentLink: undefined } }],
  ["xterm has no link detection of that name, as a later version may not", { linkifier: {} }],
  ["xterm has no such internals at all", undefined],
])("finds none where %s, so a click reaches the program as it always did", (_name, core) => {
  expect(hasLinkUnderPointer(termWith(core))).toBe(false);
});
