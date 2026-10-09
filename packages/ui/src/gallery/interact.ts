import { format, type MessageArgs, type MessageKey } from "../i18n/catalog";
import type { Language } from "../i18n/languages";

/** Everything a pressable thing is looked up among. A hand-built row is a `role="button"`, and a tag
 * in a tag group is a `role="row"`. */
const PRESSABLE =
  "button, [role=button], [role=menuitem], [role=menuitemradio], [role=tab], [role=option], [role=row], a[href]";

/** The name a control is found by: its `aria-label`, otherwise its text. */
function nameOf(element: Element): string {
  return (element.getAttribute("aria-label") ?? element.textContent ?? "").trim();
}

/** An exact name, a pattern, or a test of the name. */
export type Matcher = string | RegExp | ((name: string) => boolean);

function matches(matcher: Matcher, name: string): boolean {
  if (typeof matcher === "string") return name === matcher;
  return typeof matcher === "function" ? matcher(name) : matcher.test(name);
}

const POLL_MS = 50;
const TIMEOUT_MS = 5000;

/**
 * What a scenario's steps drive the mounted app with: they press controls the way a user would, by
 * name, so a dialog or a view that lives in component state opens through the app's own code. Names
 * are looked up in the language the window is in (`t`), so a step reads the same in every language.
 */
export interface Ui {
  /** The catalog's message in the window's language, for naming a control. */
  t: <K extends MessageKey>(key: K, ...args: MessageArgs<K>) => string;
  /** The sidebar row of the session titled exactly `title` (so "Hub" is not "Hub 2"), not its
   * actions menu — a console session's row included, since it is an ordinary session row. */
  session: (title: string) => Matcher;
  /** Presses the first control named `matcher`, waiting for it to appear. Mind a name that exists
   * twice: a tag already picked in the filter is on the sidebar's heading row (earlier in the DOM)
   * and in the filter picker, so pressing it again to unpick it would hit the sidebar row. */
  press: (matcher: Matcher) => Promise<void>;
  /** Presses `key` on the focused control, for what a user closes or confirms with the keyboard. */
  key: (key: string) => Promise<void>;
  /** Focuses the text field labelled `label`, waiting for it to appear, so `type` has one to fill. */
  focus: (label: string) => Promise<void>;
  /** Types `text` into the focused field, waiting for a field to take focus, and replaces what it holds. */
  type: (text: string) => Promise<void>;
  /** Waits for `ms`, for an animation to settle. */
  wait: (ms: number) => Promise<void>;
}

export function createUi(doc: Document, language: Language): Ui {
  const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

  const find = (matcher: Matcher): HTMLElement | undefined =>
    Array.from(doc.querySelectorAll<HTMLElement>(PRESSABLE)).find(
      (element) => !element.hasAttribute("disabled") && element.getAttribute("aria-disabled") !== "true" && matches(matcher, nameOf(element)),
    );

  const t: Ui["t"] = (key, ...args) => format(language, key, (args as [Record<string, string | number>?])[0]);

  return {
    t,
    // A session row's name begins with its whole title, so "Hub" does not also match "Hub 2".
    session: (title) => {
      const prefix = t("sidebar.session.ariaLabel", { title, agent: "\0", status: "\0" }).split("\0")[0];
      return (name) => name.startsWith(prefix);
    },
    wait,
    async key(key) {
      const target = doc.activeElement ?? doc.body;
      for (const type of ["keydown", "keyup"]) {
        target.dispatchEvent(new KeyboardEvent(type, { key, bubbles: true, cancelable: true }));
      }
      await wait(300);
    },
    async type(text) {
      let field = doc.activeElement;
      for (let waited = 0; !(field instanceof HTMLInputElement) && waited < TIMEOUT_MS; waited += POLL_MS) {
        await wait(POLL_MS);
        field = doc.activeElement;
      }
      if (!(field instanceof HTMLInputElement)) throw new Error("No text field took focus to type into");
      // React tracks the value through the element's own setter, so it is set through the prototype's.
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(field, text);
      field.dispatchEvent(new Event("input", { bubbles: true }));
      await wait(150);
    },
    async focus(label) {
      for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
        const field = Array.from(doc.querySelectorAll("input")).find(
          (input) => (input.labels?.[0]?.textContent ?? input.getAttribute("aria-label")) === label,
        );
        if (field) {
          field.focus();
          return;
        }
        await wait(POLL_MS);
      }
      throw new Error(`No text field labelled ${label}`);
    },
    async press(matcher) {
      for (let waited = 0; waited <= TIMEOUT_MS; waited += POLL_MS) {
        const element = find(matcher);
        if (element) {
          element.click();
          // The press's own effects (a menu opening, a dialog mounting) land on the next frames.
          await wait(150);
          return;
        }
        await wait(POLL_MS);
      }
      throw new Error(`Nothing to press matching ${String(matcher)}`);
    },
  };
}
