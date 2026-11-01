import { useTheme } from "@heroui/react";
import React, { createContext, useContext } from "react";

/** The user's stated intent — "system" means follow the OS, the other two are an explicit override.
 * Matches the values HeroUI's own `useTheme` persists under its `"heroui-theme"` `localStorage`
 * key, since that hook is what this module wraps rather than reimplements. */
export type ThemeChoice = "light" | "dark" | "system";

interface ThemeContextValue {
  /** The user's stated intent, for the sidebar control to show which of the three is active. */
  choice: ThemeChoice;
  /** What "system" resolves to right now, or just `choice` when it is not "system" — what
   * everything that cannot use CSS alone (the terminal's xterm instance, the report panel stays
   * deliberately out of this) needs to match the theme. */
  resolved: "light" | "dark";
  setChoice: (choice: ThemeChoice) => void;
}

const ThemeContext = createContext<ThemeContextValue | undefined>(undefined);

/** Set once the mismatch below has been warned about, so a render loop does not repeat the same
 * warning on every one of `ThemeProvider`'s re-renders. */
let warnedAboutUnexpectedTheme = false;

/** Narrows `useTheme`'s `resolvedTheme` to the two values this app actually has CSS and an xterm
 * palette for. HeroUI types it as a plain `string` and applies it to `<html>` verbatim, adding its
 * class without removing the one already there, so a stored value outside the three `ThemeChoice`s
 * leaves `<html>` carrying both a third, unstyled class and the one `index.html`'s bootstrap
 * script put there from the OS preference — and it is that one the page still paints by. The OS
 * preference is therefore what the terminal has to match too. Throwing instead would take the
 * whole app to the error screen over a cosmetic setting nothing but a hand-edited `localStorage`
 * can produce, so this warns once and continues, which at least keeps the cause findable. */
function narrowResolvedTheme(value: string | undefined): "light" | "dark" {
  if (value === "light" || value === "dark") return value;
  if (!warnedAboutUnexpectedTheme) {
    warnedAboutUnexpectedTheme = true;
    console.warn(`useTheme() resolved to unexpected theme ${JSON.stringify(value)}; following the system instead`);
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/**
 * Applies HeroUI's theme class and `data-theme` attribute to `<html>` for whichever screen is
 * mounted — `StartupScreen` included, since `main.tsx` renders it in place of `App` until a daemon
 * address is known, and the theme has to read right there too. Wraps HeroUI's `useTheme` rather
 * than tracking `matchMedia` or `localStorage` itself: that hook already persists the choice,
 * resolves "system", and (via its own layout effect) writes the DOM class before paint.
 *
 * Must wrap everything that reads `useOctoboardTheme`, and only one instance may exist — `useTheme`
 * holds its own `localStorage`-backed state, so a second instance would track its own copy of the
 * choice rather than observing this one's `setChoice` calls.
 */
export function ThemeProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const { theme, resolvedTheme, setTheme } = useTheme();
  const value: ThemeContextValue = {
    choice: theme === "light" || theme === "dark" ? theme : "system",
    // `resolvedTheme` is undefined only during SSR, which this client-only app never runs under.
    resolved: narrowResolvedTheme(resolvedTheme),
    setChoice: setTheme,
  };
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useOctoboardTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useOctoboardTheme() called outside a ThemeProvider");
  return value;
}
