// The page one scenario renders in, loaded in an iframe by the gallery shell (`shell.tsx`). It
// follows `main.tsx` — the real `App` under the same platform, language, error boundary and theme
// providers, over a fixture daemon in place of the WebSocket one. It leaves out what `main.tsx` does
// for a native shell (`NativeMenuLabelsSync`, `NativeWindowThemeSync`, revealing the window): the
// browser platform has no native menu or window. Its stored preferences are set from the
// URL before the app loads (`prepareStorage`).
import React from "react";
import ReactDOM from "react-dom/client";

import { isLanguage, type Language } from "../i18n/languages";
import { SCENARIOS } from "./fixtures";
import { installFakeTerminal } from "./fakeTerminal";
import { createUi } from "./interact";
import { prepareStorage } from "./prepare";

const params = new URLSearchParams(window.location.search);
const scenario = SCENARIOS.find((s) => s.id === params.get("scenario"));
const requestedLanguage = params.get("lang");
const language: Language = isLanguage(requestedLanguage) ? requestedLanguage : "en";
const theme = params.get("theme") === "dark" ? "dark" : "light";

if (scenario) prepareStorage(scenario, language, theme);

// What `index.html`'s bootstrap script does for the real page, so the first paint, and the error
// screen outside `ThemeProvider`, are themed.
document.documentElement.classList.add(theme);
document.documentElement.setAttribute("data-theme", theme);

installFakeTerminal();

const [{ App }, { ErrorBoundary }, { LanguageProvider }, { selectPlatform }, { PlatformProvider }, startup, store, { ThemeProvider }] =
  await Promise.all([
    import("../App"),
    import("../ErrorBoundary"),
    import("../i18n/react"),
    import("../platform"),
    import("../platform/react"),
    import("../StartupScreen"),
    import("../store"),
    import("../theme"),
  ]);
// After react-aria's own listeners are registered, as in `main.tsx` (see `focusGuard.ts`).
await import("../focusGuard");
await import("../style.css");
const { createFixtureDaemon } = await import("./fixtureDaemon");

const { DaemonFailedMessage, NoAddressMessage, StartupScreen } = startup;

/** The browser platform, with the desktop window's `windowChrome` added while the URL says `chrome=1`,
 * so what the desktop app alone offers (the window shortcuts among it, ⌃Tab between console
 * sessions) can be driven in the gallery. It also puts the top bar's drag region on, and nothing
 * else native. */
function galleryPlatform(): ReturnType<typeof selectPlatform> {
  const platform = selectPlatform();
  if (params.get("chrome") !== "1") return platform;
  return { ...platform, windowChrome: { leftInset: () => 0, subscribe: () => () => {} } };
}

function Crash(): never {
  throw new Error("Gallery: a component threw while rendering.");
}

const daemon = scenario && !scenario.startup && !scenario.viewer ? createFixtureDaemon(scenario) : undefined;
const { ViewerGallery } = scenario?.viewer ? await import("./viewerGallery") : { ViewerGallery: undefined };
const viewerSubjects = scenario?.viewer ? await scenario.viewer.subjects() : undefined;

function screen(): React.ReactElement {
  if (!scenario) return <p className="p-4">Unknown scenario “{params.get("scenario")}”.</p>;
  switch (scenario.startup?.kind) {
    case "noAddress":
      return <StartupScreen message={<NoAddressMessage />} />;
    case "daemonFailed":
      return <StartupScreen message={<DaemonFailedMessage error={scenario.startup.error} />} />;
    case "crash":
      return <Crash />;
  }
  if (viewerSubjects && ViewerGallery) return <ViewerGallery subjects={viewerSubjects} loadDelay={scenario.viewer?.loadDelay} />;
  return (
    <store.DaemonProvider value={daemon!}>
      <App />
    </store.DaemonProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <PlatformProvider value={galleryPlatform()}>
      <LanguageProvider>
        <ErrorBoundary>
          <ThemeProvider>{screen()}</ThemeProvider>
        </ErrorBoundary>
      </LanguageProvider>
    </PlatformProvider>
  </React.StrictMode>,
);

// The scenario's interactions, once the app has mounted. A failed one is shown over the page as
// well as logged, since the scenario then shows less than it says.
try {
  const ui = createUi(document, language, daemon);
  await ui.wait(300);
  for (const step of scenario?.steps ?? []) await step(ui);
} catch (error) {
  console.error(error);
  const banner = document.createElement("div");
  banner.textContent = `Scenario step failed: ${(error as Error).message}`;
  banner.style.cssText = "position:fixed;inset-inline:0;bottom:0;z-index:9999;padding:8px;background:#b91c1c;color:#fff;font:12px monospace";
  document.body.append(banner);
}
document.documentElement.dataset.galleryReady = "true";
