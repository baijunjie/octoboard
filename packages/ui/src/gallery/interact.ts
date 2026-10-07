import { format, type MessageArgs, type MessageKey } from "../i18n/catalog";
import type { Language } from "../i18n/languages";

/** Everything a pressable thing is looked up among. A hand-built row is a `role="button"`. */
const PRESSABLE = "button, [role=button], [role=menuitem], [role=menuitemradio], [role=tab], [role=option], a[href]";

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
  /** The sidebar row of the session titled `title`, not its actions menu. */
  session: (title: string) => Matcher;
  /** The console's hub row. */
  hub: () => Matcher;
  /** Presses the first control named `matcher`, waiting for it to appear. */
  press: (matcher: Matcher) => Promise<void>;
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
    session: (title) => (name) => name.includes(title) && name !== t("sidebar.session.actions", { title }),
    hub: () => {
      // The row's label is a sentence around two placeholders; what comes before the first is what
      // tells it from every other row, in any language.
      const prefix = t("sidebar.hub.ariaLabel", { agent: "\u0000", status: "\u0001" }).split("\u0000")[0];
      if (!prefix) throw new Error("The hub row's label has no fixed start to find it by");
      return (name) => name.startsWith(prefix);
    },
    wait,
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
