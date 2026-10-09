// @vitest-environment jsdom
import { act, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, type Mock, vi } from "vitest";

import type { PlatformAdapter } from "../platform";
import { PlatformProvider } from "../platform/react";
import type { Location } from "./history";
import { type Navigation, useNavigationHistory } from "./useNavigationHistory";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

interface Harness {
  navigation: Navigation;
  visit: (session: string | undefined, focus?: string) => void;
  shown: () => Location;
  focusTerminal: Mock<() => void>;
  /** A Back button as the title bar renders it: disabled when there is nothing to go back to. */
  backButton: () => HTMLButtonElement;
}

let root: Root | undefined;
let container: HTMLElement | undefined;

afterEach(() => {
  act(() => root?.unmount());
  container?.remove();
  root = container = undefined;
});

/** The UI as `App` keeps it, reduced to the selected session and the focus mode. `apply` states
 * what the UI does with a location it is told to restore: `settle` can turn it into another one. */
function mount({
  alive = () => true,
  settle = (location) => location,
  platform = { kind: "browser" },
}: {
  alive?: (location: Location) => boolean;
  settle?: (location: Location) => Location;
  platform?: PlatformAdapter;
} = {}): Harness {
  const harness = { focusTerminal: vi.fn<() => void>() } as unknown as Harness;
  function Host() {
    const [session, setSession] = useState<string>();
    const [focus, setFocus] = useState<string>();
    const location: Location = { console: "c-1", session, focus };
    harness.shown = () => location;
    harness.visit = (nextSession, nextFocus) => {
      setSession(nextSession);
      setFocus(nextFocus);
    };
    harness.navigation = useNavigationHistory({
      location,
      ready: true,
      isLive: alive,
      apply: (target) => {
        const settled = settle(target);
        setSession(settled.session);
        setFocus(settled.focus);
      },
      focusTerminal: harness.focusTerminal,
    });
    return (
      <button type="button" data-testid="back" disabled={!harness.navigation.canGoBack} onClick={harness.navigation.back}>
        Back
      </button>
    );
  }
  container = document.body.appendChild(document.createElement("div"));
  root = createRoot(container);
  act(() => root!.render(<PlatformProvider value={platform}><Host /></PlatformProvider>));
  harness.backButton = () => container!.querySelector<HTMLButtonElement>('[data-testid="back"]')!;
  return harness;
}

const visitAll = (harness: Harness, ...sessions: string[]) => {
  for (const session of sessions) act(() => harness.visit(session));
};

describe("useNavigationHistory", () => {
  it("records each visit and moves back and forward without losing the entries ahead", () => {
    const h = mount();
    visitAll(h, "a", "b", "c");
    expect(h.navigation.canGoForward).toBe(false);

    act(() => h.navigation.back());
    expect(h.shown().session).toBe("b");
    act(() => h.navigation.back());
    expect(h.shown().session).toBe("a");
    act(() => h.navigation.back());
    expect(h.shown().session).toBeUndefined();
    expect(h.navigation.canGoBack).toBe(false);
    expect(h.navigation.canGoForward).toBe(true);

    for (const session of ["a", "b", "c"]) {
      act(() => h.navigation.forward());
      expect(h.shown().session).toBe(session);
    }
    expect(h.navigation.canGoForward).toBe(false);
  });

  it("drops what was ahead once something new is visited after going back", () => {
    const h = mount();
    visitAll(h, "a", "b", "c");
    act(() => h.navigation.back());
    visitAll(h, "d");
    expect(h.navigation.canGoForward).toBe(false);
    act(() => h.navigation.back());
    expect(h.shown().session).toBe("b");
  });

  it("takes the place the UI settled at as the entry, instead of adding a visit that wipes Forward", () => {
    // Restoring a focus mode that does not hold the session leaves it, as `App` does.
    const h = mount({ settle: (target) => ({ ...target, focus: target.session === "b" ? undefined : target.focus }) });
    act(() => h.visit("a"));
    act(() => h.visit("b", "project:p-1"));
    visitAll(h, "c");

    act(() => h.navigation.back());
    expect(h.shown()).toEqual({ console: "c-1", session: "b", focus: undefined });
    expect(h.navigation.canGoForward).toBe(true);
    act(() => h.navigation.forward());
    expect(h.shown().session).toBe("c");

    // The entry now says what is shown, so going back to it again changes nothing further.
    act(() => h.navigation.back());
    expect(h.navigation.canGoBack).toBe(true);
    expect(h.navigation.canGoForward).toBe(true);
  });

  it("does not let a move that changes nothing mistake the next visit for part of itself", () => {
    // `apply` cannot express going to "a", so the UI stays on "b": the move changes nothing.
    const h = mount({ settle: (target) => (target.session === "a" ? { ...target, session: "b" } : target) });
    visitAll(h, "a", "b");
    act(() => h.navigation.back());
    expect(h.shown().session).toBe("b");
    // The entry now says "b", which is what is shown, so Forward has nowhere visibly different to go.
    expect(h.navigation.canGoForward).toBe(false);

    act(() => h.visit("z"));
    expect(h.navigation.canGoForward).toBe(false);
    act(() => h.navigation.back());
    expect(h.shown().session).toBe("b");
  });

  it("replaces the entry on screen when what it names is gone, rather than adding a visit", () => {
    const gone = new Set<string>();
    const h = mount({ alive: (location) => !location.session || !gone.has(location.session) });
    visitAll(h, "a", "b");

    // The selected session is deleted elsewhere and the UI falls back to nothing selected.
    gone.add("b");
    act(() => h.visit(undefined));
    // Had that been added as a visit, "b" (live again below) would be the entry Back goes to.
    gone.delete("b");
    act(() => h.navigation.back());
    expect(h.shown().session).toBe("a");
  });

  it("hands focus on to the terminal when a move leaves it on nothing", () => {
    const h = mount();
    visitAll(h, "a", "b");
    (document.activeElement as HTMLElement | null)?.blur();
    act(() => h.navigation.back());
    expect(h.focusTerminal).toHaveBeenCalledTimes(1);

    h.backButton().focus();
    act(() => h.navigation.forward());
    expect(h.focusTerminal).toHaveBeenCalledTimes(1);
  });

  it("hands focus on to the terminal when the move disables the button that held it", () => {
    const h = mount();
    visitAll(h, "a");
    h.backButton().focus();
    expect(document.activeElement).toBe(h.backButton());
    // Back to the first entry: nothing is left behind it, so the button is disabled by this move.
    act(() => h.navigation.back());
    expect(h.backButton().disabled).toBe(true);
    expect(h.focusTerminal).toHaveBeenCalledTimes(1);
  });

  describe("moveByShortcut", () => {
    const windowChrome = { leftInset: () => 0, subscribe: () => () => {} };

    it("does nothing where there is no window chrome", () => {
      const h = mount();
      visitAll(h, "a", "b");
      act(() => h.navigation.moveByShortcut(true));
      expect(h.shown().session).toBe("b");
    });

    it("goes back and forward where there is", () => {
      const h = mount({ platform: { kind: "browser", windowChrome } });
      visitAll(h, "a", "b");
      act(() => h.navigation.moveByShortcut(true));
      expect(h.shown().session).toBe("a");
      act(() => h.navigation.moveByShortcut(false));
      expect(h.shown().session).toBe("b");
    });

    it.each(['<div role="dialog"></div>', '<div role="menu"></div>'])("does nothing while %s is open", (markup) => {
      const h = mount({ platform: { kind: "browser", windowChrome } });
      visitAll(h, "a", "b");
      const open = document.body.appendChild(document.createElement("div"));
      open.innerHTML = markup;
      act(() => h.navigation.moveByShortcut(true));
      expect(h.shown().session).toBe("b");
      open.remove();
      act(() => h.navigation.moveByShortcut(true));
      expect(h.shown().session).toBe("a");
    });
  });
});

describe("joinPressVisits", () => {
  /** A console and a session as separate state, as a press in the previewing sidebar changes them: the
   * console on the press, the session on its release. */
  function mountConsoles() {
    let navigation!: Navigation;
    let setConsole!: (id: string) => void;
    let setSession!: (id: string) => void;
    function Host() {
      const [console_, setC] = useState("c-1");
      const [session, setS] = useState<string>("s-1");
      setConsole = setC;
      setSession = setS;
      navigation = useNavigationHistory({
        location: { console: console_, session },
        ready: true,
        isLive: () => true,
        apply: (target) => {
          setC(target.console!);
          setS(target.session!);
        },
        focusTerminal: () => {},
      });
      return null;
    }
    container = document.body.appendChild(document.createElement("div"));
    root = createRoot(container);
    act(() => root!.render(<PlatformProvider value={{ kind: "browser" }}><Host /></PlatformProvider>));
    return { navigation: () => navigation, setConsole: (id: string) => act(() => setConsole(id)), setSession: (id: string) => act(() => setSession(id)) };
  }
  const release = (type = "pointerup") => {
    window.dispatchEvent(new MouseEvent(type));
    act(() => void vi.advanceTimersByTime(100));
  };
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("records the console change and the selection that follows it before the release as one visit", () => {
    const ui = mountConsoles();
    ui.navigation().joinPressVisits();
    ui.setConsole("c-2");
    ui.setSession("s-2");
    release();
    act(() => ui.navigation().back());
    // Straight back to where the press began, not to the console changed with the old session.
    expect(ui.navigation().canGoBack).toBe(false);
    act(() => ui.navigation().forward());
    expect(ui.navigation().canGoForward).toBe(false);
  });

  it("keeps joining when a second press is announced just after the first one's release, and drops the first one's listeners", () => {
    const removed = vi.spyOn(window, "removeEventListener");
    const ui = mountConsoles();
    ui.navigation().joinPressVisits();
    window.dispatchEvent(new MouseEvent("pointerup"));
    expect(removed.mock.calls.filter(([type]) => ["pointerup", "pointercancel", "blur"].includes(type))).toHaveLength(3);
    act(() => void vi.advanceTimersByTime(20));
    ui.navigation().joinPressVisits();
    // The first release's reset timer would have fired here, ending the second press's join.
    act(() => void vi.advanceTimersByTime(40));
    ui.setConsole("c-2");
    ui.setSession("s-2");
    release();
    act(() => ui.navigation().back());
    expect(ui.navigation().canGoBack).toBe(false);
    removed.mockRestore();
  });

  it.each(["pointerup", "pointercancel", "blur"])("leaves a selection made after the press ended (%s) as a visit of its own", (type) => {
    const ui = mountConsoles();
    ui.navigation().joinPressVisits();
    ui.setConsole("c-2");
    release(type);
    ui.setSession("s-2");
    act(() => ui.navigation().back());
    expect(ui.navigation().canGoBack).toBe(true);
  });
});
