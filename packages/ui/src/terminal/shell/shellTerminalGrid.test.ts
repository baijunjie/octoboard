// @vitest-environment jsdom
import { afterEach, describe, expect, it, onTestFinished, vi } from "vitest";

import { FAKE_FRAME_MS, fakeTimers } from "../../test/clock";
import { XTERM_SCROLLBAR_WIDTH as SCROLLBAR } from "../TerminalController";
import { stubTerminalLayout } from "./shellTerminalGridTestSupport";
import { registerShellTerminalBody, shellTerminalFontSize, shellTerminalStartSize } from "./shellTerminalGrid";

const CELL = { width: 7, height: 14.5 };

let nextProject = 0;
/** A project no other test of this file used, as bodies are held by project for the life of the module. */
const freshProject = () => `project-${nextProject++}`;

/** A body in the page, registered for `project` until the test finishes. */
function body(project: string, parent: HTMLElement | null = document.body): HTMLElement {
  const element = document.createElement("div");
  parent?.append(element);
  onTestFinished(registerShellTerminalBody(project, element));
  return element;
}

afterEach(() => {
  document.body.replaceChildren();
  document.documentElement.removeAttribute("style");
});

describe("the font size", () => {
  const rootStyle = (properties: Record<string, string>) => {
    for (const [name, value] of Object.entries(properties)) document.documentElement.style.setProperty(name, value);
  };

  it("is 12 pixels when the page names no code size", () => {
    expect(shellTerminalFontSize()).toBe(12);
  });

  it("is the size `--text-xs` names, a px value as it is and a rem value against the root's font size", () => {
    rootStyle({ "--text-xs": "10px" });
    expect(shellTerminalFontSize()).toBe(10);
    rootStyle({ "--text-xs": "0.5rem", "font-size": "20px" });
    expect(shellTerminalFontSize()).toBe(10);
    rootStyle({ "--text-xs": "1.5rem" });
    expect(shellTerminalFontSize()).toBe(30);
  });

  it("is 12 pixels when the value is not a plain size, or a rem value has no root size to go by", () => {
    rootStyle({ "--text-xs": "calc(1px + 1px)" });
    expect(shellTerminalFontSize()).toBe(12);
    rootStyle({ "--text-xs": "0.75rem", "font-size": "0px" });
    expect(shellTerminalFontSize()).toBe(12);
  });
});

describe("the start size", () => {
  it.each([
    // [width, height, cell, cols, rows]: the columns are what is left of the truncated width after the scrollbar,
    // over a cell's width, less the one column kept back; the rows the height over a cell's height rounded up.
    [800, 400, CELL, 111, 26],
    [807, 405, CELL, 112, 27],
    [806, 435, CELL, 112, 29],
    [SCROLLBAR + 3 * CELL.width, 15, CELL, 2, 1],
    // The width is truncated before it is divided, as xterm's fit truncates it: 46.8 is five cells of 6.5 after the
    // scrollbar, 46 only four, of which one is kept back.
    [SCROLLBAR + 32.8, 400, { width: 6.5, height: 14.5 }, 3, 26],
  ])("is the grid a %s by %s view holds, for a cell %j: %s columns by %s rows", async (width, height, cell, cols, rows) => {
    stubTerminalLayout({ width, height, cell });
    body("p");
    await expect(shellTerminalStartSize("p")).resolves.toEqual({ cols, rows });
  });

  it.each([
    ["fewer than two columns once one is kept back", SCROLLBAR + 3 * CELL.width - 1, 400],
    ["no room beside the scrollbar", SCROLLBAR, 400],
    ["no row", 800, 14],
  ])("is nothing for a view that holds %s, so the daemon's default stands", async (_name, width, height) => {
    stubTerminalLayout({ width, height, cell: CELL });
    body("p");
    await expect(shellTerminalStartSize("p")).resolves.toBeUndefined();
  });

  it("is nothing while the body has no size", async () => {
    body("p");
    await expect(shellTerminalStartSize("p")).resolves.toBeUndefined();
    stubTerminalLayout({ width: 0, height: 400, cell: CELL });
    await expect(shellTerminalStartSize("p")).resolves.toBeUndefined();
    const layout = stubTerminalLayout({ width: 800, height: 0, cell: CELL });
    await expect(shellTerminalStartSize("p")).resolves.toBeUndefined();
    expect(layout.drawnAt).toEqual([]);
  });

  it("is nothing for a character cell with no width", async () => {
    stubTerminalLayout({ width: 800, height: 400, cell: { width: 0, height: 14 } });
    body("p");
    await expect(shellTerminalStartSize("p")).resolves.toBeUndefined();
  });

  it("is nothing for a body that has left the page", async () => {
    stubTerminalLayout({ width: 800, height: 400, cell: CELL });
    body("p", null);
    await expect(shellTerminalStartSize("p")).resolves.toBeUndefined();
  });

  it("measures the body of the project asked about", async () => {
    stubTerminalLayout({ width: 800, height: 400, cell: CELL });
    body("a");
    const detached = body("b", null);
    await expect(shellTerminalStartSize("a")).resolves.toEqual({ cols: 111, rows: 26 });
    expect(detached.isConnected).toBe(false);
    await expect(shellTerminalStartSize("b")).resolves.toBeUndefined();
  });

  it("draws the cell at the panel's code size, and leaves nothing of the measurement in the body", async () => {
    document.documentElement.style.setProperty("--text-xs", "10px");
    const layout = stubTerminalLayout({ width: 800, height: 400, cell: CELL });
    const element = body("p");
    element.append(document.createElement("span"));
    const before = element.innerHTML;
    await shellTerminalStartSize("p");
    expect(layout.drawnAt).toEqual(["10px"]);
    expect(element.innerHTML).toBe(before);
    stubTerminalLayout({ width: 1, height: 1, cell: CELL });
    await shellTerminalStartSize("p");
    expect(element.innerHTML).toBe(before);
  });
});

describe("waiting for the body", () => {
  it("answers as soon as a body registers, without waiting out the frames", async () => {
    fakeTimers();
    stubTerminalLayout({ width: 800, height: 400, cell: CELL });
    const project = freshProject();
    const asked = shellTerminalStartSize(project);
    const alsoAsked = shellTerminalStartSize(project);
    body(project);
    await expect(asked).resolves.toEqual({ cols: 111, rows: 26 });
    await expect(alsoAsked).resolves.toEqual({ cols: 111, rows: 26 });
  });

  it("gives a body that is not there two frames, then answers nothing", async () => {
    fakeTimers();
    stubTerminalLayout({ width: 800, height: 400, cell: CELL });
    let answer: unknown = "pending";
    void shellTerminalStartSize(freshProject()).then((size) => (answer = size));
    await fakeFrames(1);
    expect(answer).toBe("pending");
    await fakeFrames(1);
    expect(answer).toBeUndefined();
  });

  it("does not wait for a body already there", async () => {
    fakeTimers();
    stubTerminalLayout({ width: 800, height: 400, cell: CELL });
    const project = freshProject();
    body(project);
    // No frame is advanced: the clock is faked, so an answer here did not wait on one.
    await expect(shellTerminalStartSize(project)).resolves.toEqual({ cols: 111, rows: 26 });
  });

  it("measures the body that registered last, and a body that unregisters is forgotten only when it is the one held", async () => {
    fakeTimers();
    stubTerminalLayout({ width: 800, height: 400, cell: CELL });
    const project = freshProject();
    const first = document.body.appendChild(document.createElement("div"));
    const second = document.body.appendChild(document.createElement("div"));
    const unregisterFirst = registerShellTerminalBody(project, first);
    const unregisterSecond = registerShellTerminalBody(project, second);
    onTestFinished(unregisterSecond);
    // The earlier body's late unregister leaves the later one in place.
    unregisterFirst();
    await expect(shellTerminalStartSize(project)).resolves.toEqual({ cols: 111, rows: 26 });
    unregisterSecond();
    let answer: unknown = "pending";
    void shellTerminalStartSize(project).then((size) => (answer = size));
    await fakeFrames(2);
    expect(answer).toBeUndefined();
  });
});

/** Lets `count` frames of the fake clock pass, and the promise work that follows them. */
async function fakeFrames(count: number): Promise<void> {
  await vi.advanceTimersByTimeAsync(count * FAKE_FRAME_MS);
}
