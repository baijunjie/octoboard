import React from "react";

import { ErrorBoundary } from "./ErrorBoundary";
import { announceHost } from "./i18n/platformTerms";
import { LanguageProvider } from "./i18n/react";
import { useKeepAwakeSync } from "./lifecycle/useKeepAwakeSync";
import { useNativeMenuLabels } from "./lifecycle/useNativeMenuLabels";
import { useNativeWindowTheme } from "./lifecycle/useNativeWindowTheme";
import { ThemeProvider } from "./preferences/theme";
import type { PlatformAdapter } from "./platform";
import { PlatformProvider } from "./platform/react";
import { ErrorAlerts } from "./shell/ErrorAlerts";
import { DaemonHostAnnouncement, DaemonProvider, type Daemon } from "./store";
import { UpdatesProvider } from "./updates/UpdatesProvider";

// Mounted once, as a sibling of the screen, rather than from inside any screen: `App` is not on
// the no-daemon screens at all, and `StartupScreen` is also what `ErrorBoundary` renders as its
// fallback — which, per the ordering in `AppRoot`, is outside `ThemeProvider`, so a theme hook called from
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

// Under the daemon's provider, which it reads whether the daemon holds the computer awake from, to show on the icon,
// but outside `ErrorBoundary` too.
function KeepAwakeSync(): null {
  useKeepAwakeSync();
  return null;
}

export interface AppRootProps {
  platform: PlatformAdapter;
  /** `undefined` for a screen that has no daemon to connect to. */
  daemon: Daemon | undefined;
  /** Whether the window pushes its menu labels, theme and whether the daemon holds the computer awake to a native
   * shell. The window's entry does; a page that stands in for one in a browser (the gallery, the
   * website preview) has no shell to push to. */
  nativeSync: boolean;
  /** Passed to `UpdatesProvider`: `null` leaves out the automatic checks, for a surface that must hold a state of its
   * own (the gallery). */
  firstCheckDelayMs?: number | null;
  /** Rendered inside `UpdatesProvider` beside the screen, for a surface that drives the update store itself. */
  insideUpdates?: React.ReactNode;
  /** The screen: what the window shows under the theme. */
  children: React.ReactNode;
}

/**
 * The provider tree every page that runs the application shares.
 *
 * `UpdatesProvider` is outside `ErrorBoundary`: a crash and its Try again remount everything below the boundary, and
 * the update state (an installed update waiting for its restart, a download under way) must survive that. It is inside
 * `DaemonProvider`, since a daemon that updates itself is what it follows where the platform has no updater of its own,
 * and provides nothing where neither updates.
 *
 * `DaemonProvider` is outside `ErrorBoundary` too, so the crash page's report can remove the names of the user's
 * records (it reads them from the daemon in context); there is none for the screens that have no daemon. The daemon's
 * host is announced to the catalog beside it, outside the boundary too, so the error alerts and the crash page word
 * what happened on the daemon's machine in its terms.
 *
 * One render for every screen, so none of them is outside the boundary. `ErrorBoundary` wraps `ThemeProvider`, not the
 * other way around: `useTheme`'s state initialiser reads `localStorage` unguarded, and a throw there must still land on
 * the boundary's fallback rather than a blank window. The cost is that the fallback renders outside `ThemeProvider`
 * (see `NativeWindowThemeSync` for what that rules out); it still renders themed regardless, since `index.html`'s
 * bootstrap script already put the class on `<html>` before any of this ran. There must be only one `ThemeProvider`
 * instance (see its own comment).
 */
export function AppRoot({ platform, daemon, nativeSync, firstCheckDelayMs, insideUpdates, children }: AppRootProps): React.ReactElement {
  // Before the first message below reads it; the host does not change, so announcing it again is a no-op.
  announceHost(platform.host);
  return (
    <PlatformProvider value={platform}>
      <LanguageProvider>
        {nativeSync && <NativeMenuLabelsSync />}
        <DaemonProvider value={daemon}>
          <UpdatesProvider firstCheckDelayMs={firstCheckDelayMs}>
            {insideUpdates}
            <DaemonHostAnnouncement />
            {nativeSync && <KeepAwakeSync />}
            <ErrorBoundary>
              <ThemeProvider>
                {nativeSync && <NativeWindowThemeSync />}
                {children}
              </ThemeProvider>
            </ErrorBoundary>
          </UpdatesProvider>
        </DaemonProvider>
        <ErrorAlerts daemon={daemon} />
      </LanguageProvider>
    </PlatformProvider>
  );
}
