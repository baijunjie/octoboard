// @vitest-environment jsdom
import type { Terminal } from "@xterm/xterm";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { BROWSER_HOST, MAC_HOST, WINDOWS_HOST } from "../../test/hosts";
import { LinkClickReports } from "./linkClickReports";

/** A terminal as far as `LinkClickReports` reads it: its mouse tracking mode, and the link xterm's link detection holds
 * under the pointer (`_core.linkifier.currentLink`). */
function fakeTerminal({ tracking = "any", link = true }: { tracking?: string; link?: boolean } = {}): Terminal {
  return { modes: { mouseTrackingMode: tracking }, _core: { linkifier: { currentLink: link ? {} : undefined } } } as unknown as Terminal;
}

/** Cells 10 by 10 pixels: a point is (column * 10 + 5, row * 10 + 5), and off the screen where negative. */
const cellAt = (event: MouseEvent) => (event.clientX < 0 ? undefined : { x: Math.floor(event.clientX / 10), y: Math.floor(event.clientY / 10) });
const point = (x: number, y: number) => ({ clientX: x * 10 + 5, clientY: y * 10 + 5 });

let container: HTMLElement;
let abort: AbortController;
let sent: string[];

beforeEach(() => {
  container = document.body.appendChild(document.createElement("div"));
  abort = new AbortController();
  sent = [];
});

afterEach(() => {
  abort.abort();
  document.body.replaceChildren();
});

/**
 * The pointer in the order xterm's listeners run in (see `LinkClickReports`): the press is seen first, and then xterm
 * reports it; at the release xterm's link detection opens the link (`opened`), then reports the release, and the window's
 * own listener, last, ends the press.
 */
/** The `buttons` bit of each `button`: primary, middle, secondary. */
const PRESSED = [1, 4, 2];

function pointer(reports: LinkClickReports) {
  const report = (name: string) => reports.report(() => sent.push(name));
  return {
    /** A press of the primary button, unless `init` says otherwise, with nothing else down: `buttons` is the state after
     * the press, as browsers give it. */
    press(at = point(2, 3), init: MouseEventInit = {}) {
      const buttons = PRESSED[init.button ?? 0];
      container.dispatchEvent(new MouseEvent("mousedown", { ...at, button: 0, buttons, bubbles: true, ...init }));
      report("press");
    },
    /** A move with the primary button down, unless `buttons` says otherwise. */
    move(at: { clientX: number; clientY: number }, buttons = 1) {
      document.body.dispatchEvent(new MouseEvent("mousemove", { ...at, buttons, bubbles: true }));
      report("motion");
    },
    /** A release that leaves no button down, unless `buttons` (the state after the release) says otherwise. */
    release({ opens, buttons = 0 }: { opens: boolean; buttons?: number }) {
      if (opens) reports.opened();
      report("release");
      window.dispatchEvent(new MouseEvent("mouseup", { buttons, bubbles: true }));
    },
  };
}

function watched(term = fakeTerminal(), host = BROWSER_HOST): ReturnType<typeof pointer> & { reports: LinkClickReports } {
  const reports = new LinkClickReports(term, host, cellAt);
  reports.watch(container, abort.signal);
  return { reports, ...pointer(reports) };
}

describe("a primary click on a link", () => {
  it("reaches the program in no report where the link opens, the motion within the cell included", () => {
    const click = watched();
    click.press();
    click.move(point(2, 3));
    click.release({ opens: true });
    expect(sent).toEqual([]);
  });

  it("is sent, the press and the release in order, at the release, where no link opens", () => {
    const click = watched();
    click.reports.opened(); // Left over from no press: nothing to carry into this one.
    click.press();
    expect(sent).toEqual([]);
    click.release({ opens: false });
    expect(sent).toEqual(["press", "release"]);
  });

  it("sends the held press before the first motion out of its cell, and everything after as it comes, the release too where the link then opens", () => {
    const click = watched();
    click.press();
    click.move(point(3, 3));
    expect(sent).toEqual(["press", "motion"]);
    click.move(point(4, 3));
    expect(sent).toEqual(["press", "motion", "motion"]);
    click.release({ opens: true });
    expect(sent).toEqual(["press", "motion", "motion", "release"]);
  });

  it("sends the held press when the pointer leaves the screen", () => {
    const click = watched();
    click.press();
    click.move({ clientX: -5, clientY: 35 });
    expect(sent).toEqual(["press", "motion"]);
  });
});

describe("a press nothing is held for", () => {
  it.each<[string, () => ReturnType<typeof watched>, MouseEventInit]>([
    ["mouse tracking is off", () => watched(fakeTerminal({ tracking: "none" })), {}],
    ["no link is under the pointer", () => watched(fakeTerminal({ link: false })), {}],
    ["the button is not the primary one", () => watched(), { button: 2 }],
    ["the middle button is down", () => watched(), { button: 1 }],
    ["it is a Control-click on a Mac", () => watched(fakeTerminal(), MAC_HOST), { ctrlKey: true }],
  ])("sends its reports at once where %s", (_name, make, init) => {
    const click = make();
    click.press(point(2, 3), init);
    expect(sent).toEqual(["press"]);
    click.release({ opens: true });
    expect(sent).toEqual(["press", "release"]);
  });

  it("holds a Control-click where it is no right-click", () => {
    const click = watched(fakeTerminal(), WINDOWS_HOST);
    click.press(point(2, 3), { ctrlKey: true });
    expect(sent).toEqual([]);
  });
});

describe("a press that was not released", () => {
  it("is ended with nothing sent when a new press comes, which is held and sent on its own", () => {
    const click = watched();
    click.press();
    expect(sent).toEqual([]);
    click.press(point(5, 5));
    expect(sent).toEqual([]);
    click.release({ opens: false });
    expect(sent).toEqual(["press", "release"]);
  });

  it("is ended with nothing sent at the first motion with no button down, which goes straight through, as what follows does", () => {
    const click = watched();
    click.press();
    click.move(point(6, 1), 0);
    expect(sent).toEqual(["motion"]);
    click.move(point(6, 2), 0);
    expect(sent).toEqual(["motion", "motion"]);
  });

  it("is let go when the watch ends, nothing is sent for it, and what follows goes straight through", () => {
    const click = watched();
    click.press();
    abort.abort();
    expect(sent).toEqual([]);
    click.move(point(3, 3));
    expect(sent).toEqual(["motion"]);
  });
});

describe("a press held when the screen goes to another connection", () => {
  it("is dropped, with its drag and its release, and what follows its release goes straight through", () => {
    const click = watched();
    click.press();
    click.reports.drop();
    click.move(point(3, 3));
    click.release({ opens: false });
    expect(sent).toEqual([]);
    click.press(point(5, 5), { button: 2 });
    expect(sent).toEqual(["press"]);
  });

  it("whose release goes unseen is ended at the first motion with no button down, and the motions from there reach the program", () => {
    const click = watched();
    click.press();
    click.reports.drop();
    click.move(point(2, 3), 0);
    expect(sent).toEqual(["motion"]);
  });

  it("whose release goes unseen is ended with nothing sent by a new primary press, which is held and sent on its own", () => {
    const click = watched();
    click.press();
    click.reports.drop();
    click.press(point(5, 5));
    expect(sent).toEqual([]);
    click.release({ opens: false });
    expect(sent).toEqual(["press", "release"]);
  });
});

describe("a chord, another button pressed while the held press's button is still down", () => {
  it("sends the held press ahead of the chord's own reports, and its release where it comes", () => {
    const click = watched();
    click.press();
    click.press(point(2, 3), { button: 2, buttons: 3 });
    expect(sent).toEqual(["press", "press"]);
    click.release({ opens: false, buttons: 2 });
    click.release({ opens: false });
    expect(sent).toEqual(["press", "press", "release", "release"]);
  });

  it("sends nothing of a press being dropped, nor of the chord, until no button is down", () => {
    const click = watched();
    click.press();
    click.reports.drop();
    click.press(point(2, 3), { button: 1, buttons: 5 });
    click.move(point(4, 3), 5);
    click.release({ opens: false, buttons: 4 });
    click.move(point(5, 3), 4);
    click.release({ opens: false });
    expect(sent).toEqual([]);
    click.move(point(6, 3), 0);
    expect(sent).toEqual(["motion"]);
  });

  it("keeps dropping through a primary press made while another button is still down, until no button is", () => {
    const click = watched();
    click.press();
    click.reports.drop();
    click.press(point(2, 3), { button: 2, buttons: 3 });
    click.release({ opens: false, buttons: 2 });
    click.press(point(2, 3), { buttons: 3 });
    click.release({ opens: false, buttons: 2 });
    click.release({ opens: false });
    expect(sent).toEqual([]);
    click.move(point(6, 3), 0);
    expect(sent).toEqual(["motion"]);
  });
});

describe("a primary press on a link with another button already down", () => {
  it("is not held: the program gets its press and release, and the other button's, though the link opens", () => {
    const click = watched();
    click.press(point(2, 3), { button: 2 });
    click.press(point(2, 3), { buttons: 3 });
    expect(sent).toEqual(["press", "press"]);
    click.release({ opens: true, buttons: 2 });
    click.release({ opens: false });
    expect(sent).toEqual(["press", "press", "release", "release"]);
  });
});
