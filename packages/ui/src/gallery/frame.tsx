// The page one scenario renders in, loaded in an iframe by the gallery shell (`shell.tsx`). It
// renders the real `App` under the app's provider tree (`AppRoot`), over a fixture daemon in place
// of the WebSocket one. It passes `nativeSync={false}` to `AppRoot` and does not reveal the window:
// the browser platform has no native menu or window. It leaves out `BackgroundRunSync` too: a scenario that offers
// `backgroundRun` gets one whose `set` does nothing (`galleryPlatform`), so there is no shell to
// push the choice to. Its stored preferences are set from the URL before the app loads
// (`prepareStorage`).
import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";

import { isLanguage, type Language } from "../i18n/languages";
import { galleryFailure } from "../fixtures/banner";
import { SCENARIOS } from "./fixtures";
import { installFakeTerminal } from "../fixtures/fakeTerminal";
import { holdPrimaryModifier } from "./heldModifier";
import { afterFrames, applyDocumentTheme } from "../fixtures/pageFrame";
import { createUi, settleMotion, stuckExit } from "../fixtures/interact";
import { prepareStorage } from "./prepare";
import { LIVE_CHANGE_EVENT } from "./scenario";
import { fixtureUpdater } from "./updaterFixture";

const params = new URLSearchParams(window.location.search);
const scenario = SCENARIOS.find((s) => s.id === params.get("scenario"));
const requestedLanguage = params.get("lang");
const language: Language = isLanguage(requestedLanguage) ? requestedLanguage : "en";
const theme = params.get("theme") === "dark" ? "dark" : "light";

// The shell's Markdown style control, which the shell offers for a viewer scenario only.
const markdownStyle = scenario?.viewer ? params.get("markdown") : null;

if (scenario) prepareStorage(scenario, language, theme, markdownStyle);

applyDocumentTheme(document, theme);

installFakeTerminal();
// The shell's Held control: the new-session "+" buttons as they are while the shortcut modifier is down.
// A scenario that asks for it opens with the modifier held; `held=0` is the shell's Released over that.
const heldModifier = (params.get("held") ?? (scenario?.held ? "1" : "0")) === "1" ? holdPrimaryModifier() : undefined;

// The app's modules are imported dynamically, after `prepareStorage` above: they read stored preferences (the
// language among them) as they load, so the scenario's own choices have to be in storage first. The converse binds
// everything this file imports statically, `./fixtures`, every scenario file and whatever those import included: none
// of it may read a stored preference at module load, or the scenario opens in the previous one's language (English
// under `lang=zh-Hans`, say).
const [{ App }, { AppRoot }, { Toasts }, { selectPlatform }, startup, { useUpdates }] = await Promise.all([
  import("../App"),
  import("../AppRoot"),
  import("../shell/Toasts"),
  import("../platform"),
  import("../StartupScreen"),
  import("../updates/UpdatesProvider"),
]);
// After react-aria's own listeners are registered, as in `main.tsx` (see `focusGuard.ts`).
await import("../input/focusGuard");
await import("../style.css");
const { createFixtureDaemon } = await import("../fixtures/fixtureDaemon");
const { installReportPageFixture } = await import("./reportPageFixture");

const { DaemonFailedScreen, NoAddressMessage, StartupScreen } = startup;

/** What a file the gallery's open dialog hands back holds unless the scenario gives one: a document
 * of the right format with nothing in it. */
const EMPTY_DOCUMENT = JSON.stringify({ format: "octoboard-configuration", version: 1, consoles: [] });

/** The window's quit handler once the app has registered it, for a scenario that asks to quit. */
let requestQuit: (() => void) | undefined;
/** The window's handler of the daemon exiting by itself once the app has registered it, for a scenario that says so. */
let daemonExited: ((detail: string) => void) | undefined;

/** The browser platform, with what the desktop app alone offers added so it can be driven in the
 * gallery: `windowChrome` while the URL says `chrome=1` (the window shortcuts among it, ⌃Tab
 * between console sessions; it also puts the top bar's drag region on) and, with the scenario's
 * `desktopApp`, the desktop platform kind and native menu bar (which no menu is drawn for), the macOS privacy grants stubbed
 * from the scenario's `macosPermissions` (its System Settings pane refused when it gives a
 * `macosSettingsOpenError`), there being no other
 * way to look at those rows than to really change a system setting, a link opener that refuses with
 * the error of a scenario's `externalLinkError` (otherwise the browser's own opener stands, which
 * opens a new tab), and `backgroundRun` for a scenario that says so (the Settings switch), the
 * macOS app's system-managed `notifications` for one that says `systemNotifications` (nothing is
 * shown or opened), the clipboard taken away for a
 * scenario that says so, and the configuration-file capability stubbed from the scenario's
 * `configuration` (absent where it says the platform has none) so that no download or file picker
 * is raised from a capture, and the exit capability for a scenario that says `quitRequested`, whose
 * quit request is raised once its steps have run and whose confirmation ends nothing (or fails with
 * the text of its `exitFailure`), the daemon exiting by itself for a scenario that says
 * `daemonExited`, and the updater for a scenario that has one (`updater`), with that same exit
 * capability, since a restart goes through it. Nothing else native; `chrome=1` alone keeps the browser platform kind. */
function galleryPlatform(): ReturnType<typeof selectPlatform> {
  const platform = selectPlatform();
  const grants = scenario?.macosPermissions;
  const configuration = scenario?.configuration;
  const externalLinkError = scenario?.externalLinkError;
  let clipboardText = typeof scenario?.clipboard === "object" ? scenario.clipboard.text : "";
  return {
    ...platform,
    kind: scenario?.desktopApp ? "tauri" : platform.kind,
    ...((params.get("chrome") === "1" || scenario?.desktopApp) && { windowChrome: { leftInset: () => 0, subscribe: () => () => {} } }),
    ...(scenario?.desktopApp && { appMenu: { onSettingsRequested: () => () => {}, onAboutRequested: () => () => {}, setLabels: async () => {} } }),
    ...(grants && {
      macosPermissions: {
        status: () =>
          grants === "unreadable" ? Promise.reject(new Error("gallery: the shell has no such command")) : Promise.resolve(grants),
        request: () => Promise.resolve(false),
        openSystemSettings: () => scenario?.macosSettingsOpenError === undefined
          ? Promise.resolve()
          : Promise.reject(new Error(scenario.macosSettingsOpenError)),
      },
    }),
    ...(externalLinkError !== undefined && {
      links: { open: async () => { throw new Error(externalLinkError); } },
    }),
    ...(scenario?.updater && { updater: fixtureUpdater(scenario.updater) }),
    ...((scenario?.quitRequested || scenario?.updater || scenario?.daemonExited !== undefined || scenario?.exitFailure !== undefined) && {
      exit: {
        register: ({ onQuitRequested, onDaemonExited }) => {
          requestQuit = onQuitRequested;
          daemonExited = onDaemonExited;
          return () => { requestQuit = undefined; daemonExited = undefined; };
        },
        heartbeat: async () => {},
        confirmQuit: async () => {
          if (scenario?.exitFailure !== undefined) throw new Error(scenario.exitFailure);
        },
      },
    }),
    ...(scenario?.backgroundRun && { backgroundRun: { set: async () => {} } }),
    ...(scenario?.systemNotifications && {
      notifications: {
        ensurePermission: async () => true,
        notify: async () => {},
        openSystemSettings: async () => {},
      },
    }),
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
        save: async (suggestedName) =>
          configuration?.downloads ? { kind: "handedOver" } : { kind: "saved", name: suggestedName },
        open: async () => {
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

// A viewer scenario has one too: the viewer's header copies a path through the daemon's notifications.
const daemon = scenario && !scenario.startup ? createFixtureDaemon(scenario) : undefined;
if (daemon && scenario) installReportPageFixture(document, daemon, scenario.id);
const { ViewerGallery } = scenario?.viewer ? await import("./viewerGallery") : { ViewerGallery: undefined };
const viewerSubjects = scenario?.viewer ? await scenario.viewer.subjects(language) : undefined;
const { CodeGallery } = scenario?.codePalette ? await import("./codeGallery") : { CodeGallery: undefined };
const { LicensesGallery } = scenario?.licensesDialog ? await import("./licensesGallery") : { LicensesGallery: undefined };
const { SemanticColoursGallery } = scenario?.semanticColours ? await import("./semanticColoursGallery") : { SemanticColoursGallery: undefined };
const { ButtonsGallery } = scenario?.pendingButtons ? await import("./buttonsGallery") : { ButtonsGallery: undefined };

function screen(): React.ReactElement {
  if (!scenario) return <p className="p-4">Unknown scenario “{params.get("scenario")}”.</p>;
  if (CodeGallery) return <CodeGallery />;
  if (LicensesGallery && scenario.licensesDialog) return <LicensesGallery state={scenario.licensesDialog} />;
  if (SemanticColoursGallery) return <SemanticColoursGallery />;
  if (ButtonsGallery) return <ButtonsGallery />;
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
      <>
        <ViewerGallery subjects={viewerSubjects} loadDelay={scenario.viewer?.loadDelay} />
        <Toasts focusTerminal={() => {}} />
      </>
    );
  return <App />;
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

// No automatic update check follows on its own, which would move a settled view 30 seconds on
// (`firstCheckDelayMs={null}`): a scenario that wants one says `checked`.
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <AppRoot
      platform={galleryPlatform()}
      daemon={daemon}
      nativeSync={false}
      firstCheckDelayMs={null}
      insideUpdates={scenario?.updater?.checked && <CheckedOnOpen />}
    >
      {screen()}
    </AppRoot>
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
  if (scenario?.daemonExited !== undefined) {
    if (daemonExited) {
      daemonExited(scenario.daemonExited);
      await ui.wait(300);
    } else galleryFailure("The scenario asks for the daemon to exit but the app registered no handler for it");
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
// The scenario's own notifications are raised a fixed time after the app subscribes to them, which can be after the
// steps have ended; ready comes after they have been handed to the stack. A page whose app never subscribes would wait
// for ever, so the wait is capped, and the cap is a failure: the scenario then shows less than it says.
if (daemon && scenario?.notifications?.length) {
  const NOTIFICATIONS_LIMIT_MS = 5000;
  const raised = await Promise.race([daemon.scenarioNotificationsRaised.then(() => true), ui.wait(NOTIFICATIONS_LIMIT_MS).then(() => false)]);
  if (!raised) galleryFailure(`The scenario's notifications were not raised within ${NOTIFICATIONS_LIMIT_MS / 1000}s: nothing on the page subscribed to them`);
}
// The first wait: two frames after the last step, not at its end, since what the last step set going has usually
// been drawn by then, which is what a capture or a read of the overlay needs. The cap is short, so that a page too
// busy to draw two frames in that time goes on without them.
await afterFrames(200);
// What the steps set easing (a dialog growing to its height) is waited out, so the view captured is the
// settled one; the wait is capped, as the other is, so that a page that never settles still claims ready.
await settleMotion(document, 1000);
document.documentElement.dataset.galleryReady = "true";
