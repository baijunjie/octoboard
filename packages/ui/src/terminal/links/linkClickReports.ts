import type { Terminal } from "@xterm/xterm";

import { isSecondaryPress } from "../../input/keys";
import type { HostFacts } from "../../platform/host";
import { hasLinkUnderPointer } from "../xtermInternals";

/** A cell of the screen, as `cellAtPoint` numbers it. */
interface Cell {
  x: number;
  y: number;
}

/** A primary press on a link whose mouse reports are held back, until its release shows whether it opened the link. */
interface HeldPress {
  cell: Cell;
  reports: (() => void)[];
  /** Whether its reports go nowhere, those held and those still to come up to its release: the link opened, or the
   * screen went to another connection (see `drop`). */
  dropping: boolean;
  /** Removes the listeners that follow the press to its release. */
  listeners: AbortController;
}

/**
 * Keeps a click that opens a link from reaching the program in the terminal: under mouse reporting xterm reports the
 * click to the program as well (a program that places its cursor on a click, as an agent's input does, would then move
 * it to the link). xterm 6 reports the press on `mousedown` on its element, before the release can show whether the
 * press opens a link: its link detection opens the link hovered at the press on a `mouseup` over it, on the screen
 * element, and the release is reported after that, from a listener on the document. So the reports of a primary press
 * on a link are held back from the press on, in order, and on the release either dropped, the release's own report
 * among them, when the link opened, or sent. A press that leaves its cell, or a release with no link opened (off the
 * link, or one that opens nothing, such as a selection's), is the program's, and its reports are sent before anything
 * that follows: a drag reaches the program as it always has, with the link opening as it always has.
 *
 * What it guarantees: of the reports xterm makes, a held press's are all sent, in order and ahead of anything after
 * them, or none of them is, from the press up to the moment no button is down; so where xterm reported a button's press
 * and its release, the program gets both or neither. Only a primary press with no other button down is held, so a held
 * gesture starts with every button up. Another button pressed while the held one is still down (a chord) makes the
 * press the program's, sent before the chord's own reports. A press dropped (the link opened, or the screen went to
 * another connection) stays dropped, a chord's reports and a press made with a button still down included, until no
 * button is down. A press whose release goes unseen (the button let go outside the window, where xterm misses the
 * release too) is ended with nothing sent, at the first motion with no button down, or at the next press with no other
 * button down. A secondary press, a primary press with another button down (a click that opens a link then reaches the
 * program too), and a press with no link under the pointer or with mouse reporting off, are left alone.
 * It relies on the order xterm's listeners run in, so recheck it on every `@xterm/xterm` upgrade: if that changes, a
 * click on a link reaches the program again.
 */
/** The `buttons` bit of each `button`: primary, middle, secondary, back, forward. */
const BUTTON_BITS = [1, 4, 2, 8, 16];

/** The buttons down besides the one `event` pressed, read off `buttons`, which on a press is the state after it, the
 * pressed button included; none where the browser does not say. */
function othersDown(event: MouseEvent): number {
  return event.buttons === undefined ? 0 : event.buttons & ~(BUTTON_BITS[event.button] ?? 0);
}

export class LinkClickReports {
  private held?: HeldPress;

  constructor(
    private readonly term: Terminal,
    private readonly host: Pick<HostFacts, "os">,
    /** The cell under a mouse event, `undefined` off the screen. */
    private readonly cellAt: (event: MouseEvent) => Cell | undefined,
  ) {}

  /** Follows the presses on `container`, which holds the terminal's element, until `signal` aborts. Call it once the
   * terminal is open. */
  watch(container: HTMLElement, signal: AbortSignal): void {
    // Captured on the container, so a press is seen before xterm's own listeners, below it, report it.
    container.addEventListener("mousedown", (event) => this.press(event), { capture: true, signal });
    signal.addEventListener("abort", () => this.settle(false));
  }

  /** Sends a mouse report the terminal produced through `send`: at once, or once the press it belongs to turns out not
   * to have opened a link, or never when it did or the screen went to another connection (see `drop`). */
  report(send: () => void): void {
    if (!this.held) send();
    else if (!this.held.dropping) this.held.reports.push(send);
  }

  /** The link under the held press opened: its reports go nowhere. Call it from the link's activation. */
  opened(): void {
    if (this.held) this.held.dropping = true;
  }

  /** The screen goes to another connection (another session, or none): the held press was the one before's, so its
   * reports, those held and those up to its release, go nowhere, rather than reaching the next one without the press
   * or the release they pair with. */
  drop(): void {
    if (!this.held) return;
    this.held.dropping = true;
    this.held.reports = [];
  }

  private press(event: MouseEvent): void {
    const others = othersDown(event);
    // A press being dropped stays dropped, with any press made while a button is still down, until none is.
    if (this.held?.dropping && others !== 0) return;
    // A chord, another button pressed with the held primary one still down: the press is the program's, sent ahead of
    // the chord, so its release, still to come, is paired.
    if (this.held && event.button !== 0 && (others & 1) !== 0) {
      this.settle(true);
      return;
    }
    // A primary press, or another with the primary button up: the held press's release never came, as when the button
    // was let go outside the window, and without its release the program gets none of it.
    this.settle(false);
    // Held only with no other button down, so that nothing held pairs with a report sent before it.
    if (event.button !== 0 || others !== 0 || isSecondaryPress(event, this.host)) return;
    if (this.term.modes.mouseTrackingMode === "none" || !hasLinkUnderPointer(this.term)) return;
    const cell = this.cellAt(event);
    if (!cell) return;
    const listeners = new AbortController();
    this.held = { cell, reports: [], dropping: false, listeners };
    // On the window, which is reached before xterm's motion listener on the document while capturing, and after its
    // release listener there while bubbling: the press is sent before the first motion that makes it a drag, and the
    // release's report is held with the rest.
    window.addEventListener(
      "mousemove",
      (move) => {
        // No button down: the release went unseen, and the program gets none of the press.
        if (move.buttons === 0) return this.settle(false);
        // A press being dropped stays held until no button is down, its drag included.
        if (this.held?.dropping) return;
        const at = this.cellAt(move);
        if (!at || at.x !== cell.x || at.y !== cell.y) this.settle(true);
      },
      { capture: true, signal: listeners.signal },
    );
    window.addEventListener(
      "mouseup",
      (release) => {
        // A press being dropped stays held until the last button is up.
        if (this.held?.dropping && release.buttons) return;
        this.settle(!this.held?.dropping);
      },
      { signal: listeners.signal },
    );
  }

  /** Ends the held press, sending its reports when `send`. */
  private settle(send: boolean): void {
    const held = this.held;
    if (!held) return;
    this.held = undefined;
    held.listeners.abort();
    if (send) for (const report of held.reports) report();
  }
}
