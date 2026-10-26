// The page one scenario renders in, loaded in an iframe by the gallery shell (`shell.tsx`). It
// follows `main.tsx` — the real `App` under the same platform, language, error boundary and theme
// providers, over a fixture daemon in place of the WebSocket one. It leaves out what `main.tsx`
// does for a native shell (`NativeMenuLabelsSync`, `NativeWindowThemeSync`, revealing the window):
// the browser platform has no native menu or window. It leaves out `BackgroundRunSync` too: a
// scenario that offers `backgroundRun` gets one whose `set` does nothing (`galleryPlatform`), so
// there is no shell to push the choice to. Its stored preferences are set from the URL before the
// app loads (`prepareStorage`).
import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";

import { isLanguage, type Language } from "../i18n/languages";
import { ConfigurationFileError } from "../platform/configurationFileError";
import { galleryFailure } from "./banner";
import { SCENARIOS } from "./fixtures";
import { installFakeTerminal } from "./fakeTerminal";
import { holdPrimaryModifier } from "./heldModifier";
import { createUi, settleMotion, stuckExit } from "./interact";
import { prepareStorage } from "./prepare";
import { LIVE_CHANGE_EVENT } from "./scenario";
import { fixtureUpdater } from "./updaterFixture";

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
// The shell's Held control: the new-session "+" buttons as they are while the shortcut modifier is down.
// A scenario that asks for it opens with the modifier held; `held=0` is the shell's Released over that.
const heldModifier = (params.get("held") ?? (scenario?.held ? "1" : "0")) === "1" ? holdPrimaryModifier() : undefined;

// The app's modules are imported dynamically, after `prepareStorage` above: they read stored preferences (the
// language among them) as they load, so the scenario's own choices have to be in storage first. The converse binds
// everything this file imports statically, `./fixtures`, every scenario file and whatever those import included: none
// of it may read a stored preference at module load, or the scenario opens in the previous one's language (English
// under `lang=zh-Hans`, say).
const [{ App }, { Toasts }, { ErrorAlerts }, { ErrorBoundary }, { LanguageProvider }, { selectPlatform }, { PlatformProvider }, startup, store, { ThemeProvider }, { UpdatesProvider, useUpdates }] =
  await Promise.all([
    import("../App"),
    import("../components/Toasts"),
    import("../dialogs/ErrorAlerts"),
    import("../ErrorBoundary"),
    import("../i18n/react"),
    import("../platform"),
    import("../platform/react"),
    import("../StartupScreen"),
    import("../store"),
    import("../theme"),
    import("../updates/UpdatesProvider"),
  ]);
// After react-aria's own listeners are registered, as in `main.tsx` (see `focusGuard.ts`).
await import("../focusGuard");
await import("../style.css");
const { createFixtureDaemon } = await import("./fixtureDaemon");
const { installReportPageFixture } = await import("./reportPageFixture");

const { DaemonFailedScreen, NoAddressMessage, StartupScreen } = startup;

/** What a file the gallery's open dialog hands back holds unless the scenario gives one: a document
 * of the right format with nothing in it. */
const EMPTY_DOCUMENT = JSON.stringify({ format: "octoboard-configuration", version: 1, consoles: [] });

/** The window's quit handler once the app has registered it, for a scenario that asks to quit. */
let requestQuit: (() => void) | undefined;

/** The browser platform, with what the desktop app alone offers added so it can be driven in the
 * gallery: `windowChrome` while the URL says `chrome=1` (the window shortcuts among it, ⌃Tab
 * between console sessions; it also puts the top bar's drag region on) and, with the scenario's `desktopApp`,
 * the native menu bar (which no menu is drawn for), the macOS privacy grants
 * stubbed from the scenario's `macosPermissions`, there being no other way to look at those rows
 * than to really change a system setting, and `backgroundRun` for a scenario that says so (the
 * Settings switch), the clipboard taken away for a scenario that says so, and the configuration-file
 * capability stubbed from the scenario's `configuration` (absent where it says the platform has
 * none) so that no download or file picker is raised from a capture, and the exit capability for a
 * scenario that says `quitRequested`, whose quit request is raised once its steps have run and whose
 * confirmation ends nothing, and the updater for a scenario that has one (`updater`), with that same exit
 * capability, since a restart goes through it. Nothing else native. */
function galleryPlatform(): ReturnType<typeof selectPlatform> {
  const platform = selectPlatform();
  const grants = scenario?.macosPermissions;
  const configuration = scenario?.configuration;
  let clipboardText = typeof scenario?.clipboard === "object" ? scenario.clipboard.text : "";
  return {
    ...platform,
    ...((params.get("chrome") === "1" || scenario?.desktopApp) && { windowChrome: { leftInset: () => 0, subscribe: () => () => {} } }),
    ...(scenario?.desktopApp && { appMenu: { onSettingsRequested: () => () => {}, onAboutRequested: () => () => {}, setLabels: async () => {} } }),
    ...(grants && {
      macosPermissions: {
        status: () =>
          grants === "unreadable" ? Promise.reject(new Error("gallery: the shell has no such command")) : Promise.resolve(grants),
        request: () => Promise.resolve(false),
        openSystemSettings: () => Promise.resolve(),
      },
    }),
    ...(scenario?.externalLinks && {
      links: { open: async () => {
        if (typeof scenario.externalLinks === "object") throw new Error(scenario.externalLinks.error);
      } },
    }),
    ...(scenario?.updater && { updater: fixtureUpdater(scenario.updater) }),
    ...((scenario?.quitRequested || scenario?.updater) && {
      exit: {
        register: ({ onQuitRequested }) => {
          requestQuit = onQuitRequested;
          return () => { requestQuit = undefined; };
        },
        heartbeat: async () => {},
        confirmQuit: async () => {},
      },
    }),
    ...(scenario?.backgroundRun && { backgroundRun: { set: async () => {} } }),
    ...(scenario?.clipboard === "unavailable" && { clipboard: undefined }),
    ...(typeof scenario?.clipboard === "object" && {
      clipboard: {
        writeText: async (text: string) => { clipboardText = text; },
        readText: async () => clipboardText,
      },
    }),
    configurationFile: configuration?.unavailable
      ? undefined
      : {
        save: async (suggestedName) => {
          if (configuration?.saveError !== undefined) throw new Error(configuration.saveError);
          return configuration?.downloads ? { kind: "handedOver" } : { kind: "saved", name: suggestedName };
        },
        open: async () => {
          const readError = configuration?.readError;
          if (typeof readError === "string") throw new ConfigurationFileError("io", readError);
          if (readError !== undefined) throw new ConfigurationFileError(readError.kind);
          if (configuration?.file === "pending") return new Promise<never>(() => {});
          const file = configuration?.file ?? { name: "octoboard-configuration.json", text: EMPTY_DOCUMENT };
          return file === "dismissed" ? undefined : file;
        },
      },
  };
}

function Crash({ stack }: { stack?: string }): never {
  const error = new Error("Gallery: a component threw while rendering.");
  if (stack !== undefined) error.stack = stack;
  throw error;
}

// A viewer scenario has one too: the viewer's header copies a path through the daemon's toasts.
const daemon = scenario && !scenario.startup ? createFixtureDaemon(scenario) : undefined;
if (daemon && scenario) installReportPageFixture(document, daemon, scenario.id);
const { ViewerGallery } = scenario?.viewer ? await import("./viewerGallery") : { ViewerGallery: undefined };
const viewerSubjects = scenario?.viewer ? await scenario.viewer.subjects(language) : undefined;
const { CodeGallery } = scenario?.codePalette ? await import("./codeGallery") : { CodeGallery: undefined };
const { LicensesGallery } = scenario?.licensesDialog ? await import("./licensesGallery") : { LicensesGallery: undefined };
const { SemanticColoursGallery } = scenario?.semanticColours ? await import("./semanticColoursGallery") : { SemanticColoursGallery: undefined };

function screen(): React.ReactElement {
  if (!scenario) return <p className="p-4">Unknown scenario “{params.get("scenario")}”.</p>;
  if (CodeGallery) return <CodeGallery />;
  if (LicensesGallery && scenario.licensesDialog) return <LicensesGallery state={scenario.licensesDialog} />;
  if (SemanticColoursGallery) return <SemanticColoursGallery />;
  switch (scenario.startup?.kind) {
    case "noAddress":
      return <StartupScreen message={<NoAddressMessage />} />;
    case "daemonFailed":
      return <DaemonFailedScreen error={scenario.startup.error} />;
    case "crash":
      return <Crash stack={scenario.startup.stack} />;
  }
  if (viewerSubjects && ViewerGallery)
    return (
      <store.DaemonProvider value={daemon!}>
        <ViewerGallery subjects={viewerSubjects} loadDelay={scenario.viewer?.loadDelay} />
        <Toasts focusTerminal={() => {}} />
      </store.DaemonProvider>
    );
  return (
    <store.DaemonProvider value={daemon!}>
      <App />
    </store.DaemonProvider>
  );
}

/** Runs the automatic check a scenario with `updater.checked` says has already happened, once the window is up, as
 * the provider's own first one would shortly after launch. Renders nothing. */
function CheckedOnOpen(): null {
  const store = useUpdates()?.store;
  useEffect(() => {
    void store?.check("automatic");
  }, [store]);
  return null;
}

// Around the screens and the error boundary, as `main.tsx` has it, so the update notice and the Settings rows find
// the store. No automatic check follows on its own, which would move a settled view 30 seconds on: a scenario that
// wants one says `checked`.
function withUpdates(content: React.ReactElement): React.ReactElement {
  return (
    <UpdatesProvider firstCheckDelayMs={null}>
      {scenario?.updater?.checked && <CheckedOnOpen />}
      {content}
    </UpdatesProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <PlatformProvider value={galleryPlatform()}>
      <LanguageProvider>
        {withUpdates(
          <ErrorBoundary>
            <ThemeProvider>{screen()}</ThemeProvider>
          </ErrorBoundary>,
        )}
        <ErrorAlerts daemon={daemon} />
      </LanguageProvider>
    </PlatformProvider>
  </React.StrictMode>,
);

/** Paints a failure over the page if something that was closing never left it. */
async function reportStuckExit(): Promise<void> {
  const stuck = await stuckExit(document);
  if (stuck) galleryFailure(`Something that was closing never left the page: ${stuck} is still marked data-exiting`);
}

/** Applies the scenario's live changes one at a time, in the order asked for: each is a step that waits for the
 * collection it changes to move, and two at once would act on a view still in motion. A failure is shown over the
 * page like a step's. */
class LiveChanges {
  /** Held until `release`: a change asked for while the steps run waits for them. */
  private queue: Promise<void>;
  /** Lets the changes run: called once the steps have, or have failed. */
  release!: () => void;
  constructor(private readonly ui: ReturnType<typeof createUi>) {
    this.queue = new Promise<void>((resolve) => (this.release = resolve));
  }

  apply(id: string): Promise<void> {
    this.queue = this.queue.then(async () => {
      try {
        const change = scenario?.liveChanges?.find((candidate) => candidate.id === id);
        if (!change) throw new Error(`The scenario has no live change “${id}”`);
        for (const step of change.steps) await step(this.ui);
        await settleMotion(document, 1000);
        await reportStuckExit();
      } catch (error) {
        galleryFailure(`Live change failed: ${(error as Error).message}`, error);
      }
    });
    return this.queue;
  }
}

// The scenario's interactions, once the app has mounted. A failed one is shown over the page as
// well as logged, since the scenario then shows less than it says; so is an exit that does not end.
const ui = createUi(document, language, daemon);
// Listening before the steps, so the buttons work on a page whose steps failed; a change waits for them.
const live = new LiveChanges(ui);
document.addEventListener(LIVE_CHANGE_EVENT, (event) => void live.apply(String((event as CustomEvent<unknown>).detail)));
try {
  await ui.wait(300);
  for (const step of scenario?.steps ?? []) await step(ui);
  if (scenario?.quitRequested) {
    if (requestQuit) {
      requestQuit();
      await ui.wait(300);
    } else galleryFailure("The scenario asks for a quit request but the app registered no quit handler");
  }
  // The live changes the URL asks for, as a viewer would have pressed them; the shell's buttons send the rest.
  live.release();
  for (const id of (params.get("live") ?? "").split(",").filter(Boolean)) await live.apply(id);
  // After the steps, which find the "+" buttons by their plain names, not the held form's.
  heldModifier?.press();
  await reportStuckExit();
} catch (error) {
  galleryFailure(`Scenario step failed: ${(error as Error).message}`, error);
} finally {
  live.release();
}
// The first wait: two frames after the last step, not at its end, since what the last step set going (a render, a
// transition's first frames, a failure line) has usually been drawn by then, which is what a capture or a read of
// the overlay needs. It is capped at 200ms, though, so that a tab drawing no frames (a background one) cannot hold
// it forever: it ends at two frames or at 200ms, whichever is first, and a page too busy to draw two frames in
// that time goes on without them.
await new Promise<void>((resolve) => {
  requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  setTimeout(resolve, 200);
});
// What the steps set easing (a dialog growing to its height) is waited out, so the view captured is the
// settled one; the wait is capped, as the other is, so that a page that never settles still claims ready.
await settleMotion(document, 1000);
document.documentElement.dataset.galleryReady = "true";
