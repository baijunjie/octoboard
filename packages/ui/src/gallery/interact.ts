import { format, type MessageArgs, type MessageKey } from "../i18n/catalog";
import type { Language } from "../i18n/languages";
import { DORMANT_STATUSES, LIVE_STATUSES } from "../protocol";
import { activityLabel, statusLabel } from "../sessionLabel";
import { ACTIVITIES } from "../sidebar/order";
import type { Daemon } from "../store";
import type { StatusKey } from "../viewer/statusMarks";

/** Everything a pressable thing is looked up among. A hand-built row is a `role="button"`, a tag
 * in a tag group is a `role="row"`, and a checkbox is its (visually hidden) input: pressing the label
 * HeroUI draws it in does not toggle it. */
const PRESSABLE =
  "button, [role=button], [role=menuitem], [role=menuitemradio], [role=tab], [role=option], [role=row], a[href], input[type=checkbox]";

/** The name a control is found by: its `aria-label`, otherwise its text, and for a checkbox the text
 * of its label. Deliberately not the browser's accessible-name computation, which folds in the label
 * of a `role="img"` child — such a child inside a control does not enter the name a `Matcher` is
 * handed. */
function nameOf(element: Element): string {
  const label = element instanceof HTMLInputElement ? element.labels?.[0]?.textContent : undefined;
  return withoutIsolates(element.getAttribute("aria-label") ?? label ?? element.textContent ?? "").trim();
}

/** A name without its bidi isolate characters (U+2066 to U+2069). The app puts them round a value
 * substituted into a sentence, and they are invisible, so a step names a control without them. */
function withoutIsolates(text: string): string {
  return text.replace(/[\u2066-\u2069]/g, "");
}

/** An exact name or a test of the name. There is no pattern: a pattern is written in one human
 * language and matches nothing in the others, and a name that is not copy (a branch, a file) is a
 * string or a test of its own. */
export type Matcher = string | ((name: string) => boolean);

/** A test of the name that says what it looks for, which is what a failed step reports in place of
 * the test's source. */
export const describing = (description: string, test: (name: string) => boolean): Matcher =>
  Object.assign(test, { toString: () => description });

function matches(matcher: Matcher, name: string): boolean {
  return typeof matcher === "string" ? name === matcher : matcher(name);
}

/** Whether a control can be pressed. */
const isEnabled = (element: Element): boolean => !element.hasAttribute("disabled") && element.getAttribute("aria-disabled") !== "true";

const POLL_MS = 50;
const TIMEOUT_MS = 5000;

/** HeroUI's default tooltip delay (`--tooltip-delay`), which the app leaves as it is: a tooltip
 * opens this long after the pointer comes to rest on its trigger. */
const TOOLTIP_DELAY_MS = 1500;
/** Left after the delay for the tooltip to mount, be placed and finish fading in. */
const TOOLTIP_SHOW_MS = 500;

/**
 * What a scenario's steps drive the mounted app with: they press controls the way a user would, by
 * name, so a dialog or a view that lives in component state opens through the app's own code. Names
 * are looked up in the language the window is in (`t`), so a step reads the same in every language.
 * A step may also move the fixture daemon itself (`dropConnection`), for a state the app's own
 * controls cannot be driven into.
 */
export interface Ui {
  /** The catalog's message in the window's language, for naming a control. */
  t: <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => string;
  /** The sidebar row of the session titled exactly `title` (so "Hub" is not "Hub 2"), not its
   * actions menu — a console session's row included, since it is an ordinary session row. */
  session: (title: string) => Matcher;
  /** The row of the change to the file `name` in the Git mode's change list, whatever else it says
   * after its status (where a rename came from, that it is selected). `name` is the file's display
   * base name (`wireBaseName`, so an escaped `%25` is written `%`) and `status` the kind of change,
   * which the row says in the window's language. Rows are told apart by name and status only: a file
   * that is both staged and unstaged has two rows named alike, and this takes the first. */
  changeRow: (name: string, status: StatusKey) => Matcher;
  /** Presses the control named `matcher`, waiting for it to appear: the first in document order,
   * except that a control standing inside another of the same name wins over the one around it (a
   * section heading is a `role="row"` band holding the button that folds it, and pressing the band
   * does nothing). Mind a name that exists twice side by side: a tag already picked in the filter is
   * on the sidebar's heading row (earlier in the DOM) and in the filter picker, so pressing it
   * again to unpick it would hit the sidebar row. */
  press: (matcher: Matcher) => Promise<void>;
  /** Presses the button that submits the open dialog, waiting for it to appear and be enabled: for a
   * dialog whose confirming button is named as a control behind it, which `press` would find first.
   * A disabled button (`isDisabled`, such as Confirm before a `typeToConfirm` word is typed) is
   * waited out by its native `disabled` attribute, which HeroUI sets beside `data-disabled`. */
  confirm: () => Promise<void>;
  /** Moves focus onto the scroll region named `name`, with the keyboard as its source, so that the
   * focus ring is drawn as it is for a user who tabbed there. Some regions are always there (the
   * crash screen's stack, the file viewer's reading surfaces); others only while their content
   * overflows the window, and are not found in a window tall enough to hold it: a dialog's body,
   * named by the dialog's title, and the startup screen's message. A step cannot press Tab itself:
   * a script's key event moves nothing. */
  tabToRegion: (name: Matcher) => Promise<void>;
  /** Moves the pointer onto the indicator inside the first control named `host`, waiting for the
   * control to appear, and leaves it there, for the tooltip that shows only while the pointer is
   * on it. It returns once the app's tooltip delay has passed and the tooltip is up. The indicator
   * is the control's first `role="img"` descendant, which `nameOf` leaves out of the control's
   * name, so `host` names the control as everywhere else. It dispatches no matching `pointerout`,
   * so a second call leaves both indicators hovered. It drives react-aria's hover state
   * (`data-hovered`, `isHovered`) and not CSS `:hover`: what is built with a `hover:` or
   * `group-hover/<name>:` utility shows nothing under it, and the scenario passes all the same. */
  hoverIndicator: (host: Matcher) => Promise<void>;
  /** Moves the pointer onto the status glyph of the sidebar session row named `row`
   * (`session(title)`) and leaves it there, as `hoverIndicator` does. The row's glyph is decorative,
   * so it has no `role="img"` for `hoverIndicator` to find; it is taken as the row's first element
   * child when that is a tooltip trigger, which is how `SessionRow` lays it out. A row whose glyph
   * carries no tooltip (focus mode's card) fails rather than match the agent mark's trigger further
   * along. */
  hoverRowStatus: (row: Matcher) => Promise<void>;
  /** Moves the pointer onto the status glyph of the session titled `title` in the open confirmation
   * dialog's callout list, and leaves it there, as `hoverIndicator` does. Unlike a sidebar row's,
   * that glyph is the named `role="img"` and the list row's first element child; the agent icon
   * beside it has no tooltip. Fails while no dialog lists such a session, and also when the tooltip
   * that opens is not a status's: a glyph drawn without a tooltip is the same `role="img"` first
   * child, so the step would otherwise pass over one that opens nothing. */
  hoverCalloutStatus: (title: string) => Promise<void>;
  /** Moves the pointer onto the status glyph at the end of the top bar's breadcrumb and leaves it
   * there, as `hoverIndicator` does. The glyph's tooltip trigger is the one named `role="img"`
   * in the bar; fails while no session is selected. */
  hoverTopBarStatus: () => Promise<void>;
  /** Moves the pointer onto the activity marker of the collapsed project row or the focus mode
   * switch strip chip named `host`, and leaves it there, as `hoverIndicator` does. The marker is
   * decorative and the last tooltip trigger in either, so it is taken as that; the step fails if
   * the tooltip that opens is not one of the three activities' (an expanded row, which draws no
   * marker, has none to hover), rather than leave whatever else was hovered up. */
  hoverActivityMarker: (host: Matcher) => Promise<void>;
  /** Moves focus onto the first control named `matcher`, waiting for it to appear, with the keyboard
   * as its source, so that the focus ring is drawn as it is for a user who tabbed there, and a key
   * pressed next goes to it: for a row of a list, which a press would open rather than focus. */
  focusControl: (matcher: Matcher) => Promise<void>;
  /** Presses `key` on the focused control, for what a user closes or confirms with the keyboard. */
  key: (key: string) => Promise<void>;
  /** Focuses the text field labelled `label`, waiting for it to appear, so `type` has one to fill. */
  focus: (label: string) => Promise<void>;
  /** Types `text` into the focused field, waiting for a field to take focus, and replaces what it holds. */
  type: (text: string) => Promise<void>;
  /** Waits for `ms`, for an animation to settle. */
  wait: (ms: number) => Promise<void>;
  /** Drops the control connection with its automatic attempts already spent — the banner's Retry
   * offer — for a state only reachable once the window is showing something: a scenario that
   * starts at `closed` answers no request, so nothing a step would open over it, the file viewer
   * among them, ever lists. */
  dropConnection: () => Promise<void>;
}

/** What react-aria marks `data-exiting` while it plays an element's exit transition, and removes the
 * element when that ends. */
const EXITING = "[data-exiting]";

/**
 * Waits for what is closing to be gone, and says what stays if something does. An exit transition
 * that never ends leaves the thing it was removing on screen for good (a settings section left
 * behind by the one picked after it, because a scroll-driven animation on the panel never finishes),
 * and a capture taken afterwards looks like any other, so the page reports it instead.
 */
export async function stuckExit(doc: Document): Promise<string | undefined> {
  for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
    if (!doc.querySelector(EXITING)) return undefined;
    await new Promise<void>((resolve) => setTimeout(resolve, POLL_MS));
  }
  const stuck = doc.querySelector(EXITING);
  if (!stuck) return undefined;
  const text = (stuck.textContent ?? "").trim().replace(/\s+/g, " ").slice(0, 40);
  return `<${stuck.tagName.toLowerCase()}> beginning “${text}”`;
}

export function createUi(doc: Document, language: Language, daemon?: Daemon): Ui {
  const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  /** Polls for what `look` finds, up to the timeout, so a step can name something a press or a
   * render has not yet brought into the page. */
  const waitFor = async <T>(look: () => T | undefined, missing: string): Promise<T> => {
    for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
      const found = look();
      if (found !== undefined) return found;
      await wait(POLL_MS);
    }
    throw new Error(missing);
  };

  /** Clicks what `look` finds once it is there. */
  const clickWhenFound = async (look: () => HTMLElement | undefined, missing: string): Promise<void> => {
    const element = await waitFor(look, missing);
    element.click();
    // A submenu a click opens closes again shortly unless the pointer is on its trigger, which a
    // click never places. Put it there afterwards — before the click, the same events do not hold
    // it open — so the submenu stays to be seen or pressed, as it does for a pointer.
    if (element.getAttribute("aria-haspopup") === "menu") {
      const box = element.getBoundingClientRect();
      const init: PointerEventInit = {
        bubbles: true,
        cancelable: true,
        pointerType: "mouse",
        clientX: box.x + box.width / 2,
        clientY: box.y + box.height / 2,
      };
      for (const type of ["pointermove", "pointerover", "pointerenter"]) element.dispatchEvent(new PointerEvent(type, init));
    }
    // The press's own effects (a menu opening, a dialog mounting) land on the next frames.
    await wait(150);
  };

  const find = (matcher: Matcher): HTMLElement | undefined => {
    const named = Array.from(doc.querySelectorAll<HTMLElement>(PRESSABLE)).filter((element) => isEnabled(element) && matches(matcher, nameOf(element)));
    // A control that stands inside another of the same name is the one to press: a section heading
    // is a `role="row"` band holding the button that folds it, and the band does nothing.
    return named.find((element) => !named.some((other) => other !== element && element.contains(other)));
  };

  /** Puts the pointer on what `look` finds, waiting for it to appear, and waits for its tooltip. */
  const hover = async (look: () => HTMLElement | null | undefined, missing: string): Promise<void> => {
    const indicator = await waitFor(() => look() ?? undefined, missing);
    // A tooltip opens only for a pointer, and react-aria takes the modality from the last event it
    // saw: the presses before it were clicks, which it reads as virtual. React raises
    // `onPointerEnter` from a bubbling `pointerover`.
    for (const type of ["pointermove", "pointerover"]) {
      indicator.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerType: "mouse" }));
    }
    await wait(TOOLTIP_DELAY_MS + TOOLTIP_SHOW_MS);
  };

  const t: Ui["t"] = (key, ...args) => format(language, key, (args as [Record<string, string | number>?])[0]);

  return {
    t,
    // A session row's name begins with its whole title, so "Hub" does not also match "Hub 2"; the
    // title comes before the agent and the status in every catalog, which `catalog.test.ts` holds.
    session: (title) => {
      const prefix = t("sidebar.session.ariaLabel", { title, agent: "\0", status: "\0" }).split("\0")[0];
      return describing(`session(${title})`, (name) => name.startsWith(prefix));
    },
    // Every form of a change row's name begins with `git.row`'s text, in every catalog, which is why
    // a prefix and not an equality; `catalog.test.ts` holds each catalog to it.
    changeRow: (name, status) => {
      const prefix = t("git.row", { name, status: t(`git.status.${status}`) });
      return describing(`changeRow(${name}, ${status})`, (label) => label.startsWith(prefix));
    },
    wait,
    async dropConnection() {
      if (!daemon) throw new Error("This scenario has no daemon behind it to drop the connection to");
      daemon.store.setState({ connectionState: "closed" });
      await wait(300);
    },
    async key(key) {
      const target = doc.activeElement ?? doc.body;
      for (const type of ["keydown", "keyup"]) {
        target.dispatchEvent(new KeyboardEvent(type, { key, bubbles: true, cancelable: true }));
      }
      await wait(300);
    },
    async focusControl(matcher) {
      const control = await waitFor(() => find(matcher), `Nothing to focus matching ${String(matcher)}`);
      // react-aria reads what the focus came from off the last key event, so Tab is sent first.
      (doc.activeElement ?? doc.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true }));
      control.focus();
      await wait(150);
    },
    async tabToRegion(matcher) {
      for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
        const region = Array.from(doc.querySelectorAll<HTMLElement>("[role=region][tabindex]")).find((element) => {
          const label = element.getAttribute("aria-labelledby");
          const name = label ? doc.getElementById(label)?.textContent : element.getAttribute("aria-label");
          return name != null && matches(matcher, name.trim());
        });
        if (region) {
          // react-aria reads what the focus came from off the last key event, so Tab is sent first.
          (doc.activeElement ?? doc.body).dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", bubbles: true, cancelable: true }));
          region.focus();
          await wait(150);
          return;
        }
        await wait(POLL_MS);
      }
      throw new Error(`No scroll region named ${String(matcher)}: check the name, and for a region that exists only while its content overflows (a dialog's body, the startup screen's message) that the window is short enough for that`);
    },
    async type(text) {
      const field = await waitFor(
        () => (doc.activeElement instanceof HTMLInputElement ? doc.activeElement : undefined),
        "No text field took focus to type into",
      );
      // React tracks the value through the element's own setter, so it is set through the prototype's.
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(field, text);
      field.dispatchEvent(new Event("input", { bubbles: true }));
      await wait(150);
    },
    async focus(label) {
      const field = await waitFor(
        () =>
          Array.from(doc.querySelectorAll("input")).find(
            (input) => withoutIsolates(input.labels?.[0]?.textContent ?? input.getAttribute("aria-label") ?? "") === label,
          ),
        `No text field labelled ${label}`,
      );
      field.focus();
    },
    async hoverIndicator(host) {
      await hover(() => find(host)?.querySelector<HTMLElement>("[role=img]"), `Nothing to hover an indicator of, matching ${String(host)}`);
    },
    async hoverRowStatus(row) {
      await hover(() => {
        const found = find(row);
        if (!found) return undefined;
        const glyph = found.firstElementChild;
        // Found, so waiting would not change it: the row's glyph is one without a tooltip.
        if (!glyph?.matches("[role=presentation]")) {
          throw new Error(`The session row matching ${String(row)} has no tooltip on its status glyph to hover`);
        }
        return glyph as HTMLElement;
      }, `No session row matching ${String(row)} to hover the status glyph of`);
    },
    async hoverCalloutStatus(title) {
      await hover(() => {
        const row = Array.from(doc.querySelectorAll<HTMLElement>("[role=alertdialog] li, [role=dialog] li")).find(
          (item) => item.lastElementChild?.textContent?.trim() === title,
        );
        return row?.firstElementChild?.matches("[role=img]") ? (row.firstElementChild as HTMLElement) : undefined;
      }, `No session titled ${title} in a dialog's callout list with a status glyph to hover`);
      // A glyph without `withTooltip` is the same `role="img"` first child, and hovering it opens nothing.
      const shown = doc.querySelector("[role=tooltip]")?.textContent;
      const statuses = [...LIVE_STATUSES, ...DORMANT_STATUSES];
      if (!shown || !statuses.some((status) => statusLabel(t, status) === shown)) {
        throw new Error(`The status glyph of ${title} has no tooltip: its last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async hoverTopBarStatus() {
      await hover(() => doc.querySelector<HTMLElement>("[data-region=topbar] [role=img]"), "No status glyph in the top bar");
    },
    async hoverActivityMarker(host) {
      await hover(() => {
        const found = find(host);
        if (!found) return undefined;
        return Array.from(found.querySelectorAll<HTMLElement>("[role=presentation]")).at(-1);
      }, `No row or chip matching ${String(host)} with an activity marker to hover`);
      const shown = doc.querySelector("[role=tooltip]")?.textContent;
      if (!shown || !ACTIVITIES.some((activity) => activityLabel(t, activity) === shown)) {
        throw new Error(`The control matching ${String(host)} has no activity marker: its last tooltip is ${shown ? `"${shown}"` : "none"}`);
      }
    },
    async press(matcher) {
      await clickWhenFound(() => find(matcher), `Nothing to press matching ${String(matcher)}`);
    },
    async confirm() {
      await clickWhenFound(
        () =>
          Array.from(
            doc.querySelectorAll<HTMLElement>('[role="dialog"] button[type="submit"], [role="alertdialog"] button[type="submit"]'),
          ).find(isEnabled),
        "No dialog has a button to confirm it with",
      );
    },
  };
}
