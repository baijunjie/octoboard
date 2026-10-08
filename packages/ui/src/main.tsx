import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import { suppressNativeContextMenu } from "./contextMenuGuard";
import { resolveDaemonOrigin, resolveStartupError } from "./daemon";
import { ErrorBoundary } from "./ErrorBoundary";
import { LanguageProvider } from "./i18n/react";
import { useNativeMenuLabels } from "./lifecycle/useNativeMenuLabels";
import { useNativeWindowTheme } from "./lifecycle/useNativeWindowTheme";
import { selectPlatform } from "./platform";
import { PlatformProvider } from "./platform/react";
import { DaemonFailedMessage, NoAddressMessage, StartupScreen } from "./StartupScreen";
import { createDaemon, DaemonProvider } from "./store";
import { ThemeProvider } from "./theme";
import "./focusGuard";
import "./style.css";

const rootEl = document.getElementById("root")!;
const root = ReactDOM.createRoot(rootEl);

const platform = selectPlatform();
suppressNativeContextMenu(platform);

// Where the shell puts a translucent material behind a transparent webview (the macOS application,
// `open_main_window` in `apps/desktop/src-tauri/src/lib.rs`), the page leaves the window chrome's
// background clear and paints the content panel opaquely (`style.css`, `ContentPanel`). A browser
// has no material, and keeps an opaque chrome colour.
if (platform.translucentWindow) document.documentElement.dataset.windowMaterial = "translucent";

// The native window is created hidden (`open_main_window` in
// `apps/desktop/src-tauri/src/lib.rs`), so that AppKit never gets to paint it in the OS's own
// appearance before this page has painted anything themed — nothing CSS can reach fixes a flash
// in the window's own chrome. Pushing the theme and revealing the window happen here, imperatively,
// rather than from a component: the error path renders `ErrorBoundary`'s fallback *outside*
// `ThemeProvider` (see the comment on `root.render` below), so a hook mounted under it, like
// `useNativeWindowTheme`, never runs there — and the window still has to end up both themed and
// visible on that path, same as on every other one.
//
// The theme pushed is the choice `index.html`'s bootstrap script already resolved onto `<html>`
// before this module ran, read back from there rather than re-derived from `localStorage` a
// second time in this file (see that script's own comment for why). `nativeWindow.setTheme` wants
// `null` for "follow the OS", not the value "system" happened to resolve to at this moment, so the
// raw choice decides which of the two it gets, not the resolved class alone.
const initialThemeChoice = document.documentElement.dataset.themeChoice;
const initialResolvedTheme = document.documentElement.dataset.theme;
const initialNativeThemePush = platform.nativeWindow
  ?.setTheme(initialThemeChoice === "system" ? undefined : (initialResolvedTheme as "light" | "dark"))
  .catch(() => {});

const startupError = resolveStartupError();
const daemonOrigin = resolveDaemonOrigin(platform);
// Created here rather than by a component: the connection lives as long as the window, and a
// component that owned it would drop it, for good, on its first unmount (StrictMode's included).
const daemon = !startupError && daemonOrigin ? createDaemon(daemonOrigin) : undefined;

function screen(): React.ReactElement {
  if (startupError) {
    // The daemon failed to start before the window even opened, and the shell passed the message
    // along via `?error=` instead of `?port=` — there is nothing to connect to, same as the
    // missing-address case below, just with a specific reason instead of a generic one.
    return <StartupScreen message={<DaemonFailedMessage error={startupError} />} />;
  }
  if (!daemon) {
    // No shell handed us a `?port=`, no `VITE_DAEMON_PORT` was set, and the page's own origin is
    // not one that can be the daemon — there is nothing to connect to, so say so instead of
    // guessing a port.
    return <StartupScreen message={<NoAddressMessage />} />;
  }
  return (
    <DaemonProvider value={daemon}>
      <App />
    </DaemonProvider>
  );
}

// Mounted once, as a sibling of `screen()`, rather than from inside either screen: `App` is not on
// the no-daemon screens at all, and `StartupScreen` is also what `ErrorBoundary` renders as its
// fallback — which, per the ordering below, is outside `ThemeProvider`, so a theme hook called from
// there would throw while the boundary rendered its own fallback, with nothing above left to catch
// it and a blank window as the result. Here it only ever renders under `ThemeProvider`, and the
// fallback path simply leaves the window on its last pushed appearance.
function NativeWindowThemeSync(): null {
  useNativeWindowTheme();
  return null;
}

// Mounted under `LanguageProvider` but outside `ErrorBoundary`, so the menu follows the language
// on the error screen too; unlike the theme hook it needs nothing from `ThemeProvider`.
function NativeMenuLabelsSync(): null {
  useNativeMenuLabels();
  return null;
}

// One render for every screen, so none of them is outside the boundary. `ErrorBoundary` wraps
// `ThemeProvider`, not the other way around: `useTheme`'s state initialiser reads `localStorage`
// unguarded, and a throw there must still land on the boundary's fallback rather than a blank
// window. The cost is that the fallback renders outside `ThemeProvider` (see `NativeWindowThemeSync`
// above for what that rules out); it still renders themed regardless, since `index.html`'s
// bootstrap script already put the class on `<html>` before any of this ran. There must be only
// one `ThemeProvider` instance (see its own comment).
root.render(
  <React.StrictMode>
    <PlatformProvider value={platform}>
      <LanguageProvider>
        <NativeMenuLabelsSync />
        <ErrorBoundary>
          <ThemeProvider>
            <NativeWindowThemeSync />
            {screen()}
          </ThemeProvider>
        </ErrorBoundary>
      </LanguageProvider>
    </PlatformProvider>
  </React.StrictMode>,
);

// Reveals the window, now that `render()` above has committed a themed screen into it — a top-level
// call rather than an effect inside the tree, for the same reason the initial theme push above is
// one: it must run whichever of the tree's screens ended up mounted, `ErrorBoundary`'s fallback
// included, so a crash during the very first render still ends with a visible window rather than
// one stuck hidden forever behind the shell's own, much longer, `REVEAL_SAFETY_NET` timeout (see
// `open_main_window` in `apps/desktop/src-tauri/src/lib.rs`).
//
// Waiting for a painted frame first is not available here, however much it would suit: WebKit
// suspends a hidden page's animation-frame callbacks entirely, so a `requestAnimationFrame` gate
// in front of this call can only open once something *else* has shown the window — measured as
// zero callbacks in 1200 ms, then the first one ~20 ms after an unrelated show. Gating on a frame
// would leave every launch waiting out the shell's whole `REVEAL_SAFETY_NET`. It costs nothing
// real: the page does paint while hidden (only the callbacks are suspended), so what is revealed
// here has already rendered.
//
// `initialNativeThemePush` is still waited on. Revealing before that IPC round trip settles would
// only move the flash from "before this page painted" to "before the window's appearance matched it".
void Promise.resolve(initialNativeThemePush).then(() => platform.nativeWindow?.reveal().catch(() => {}));
