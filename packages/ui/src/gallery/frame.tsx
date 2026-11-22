// The page one scenario renders in, loaded in an iframe by the gallery shell (`shell.tsx`). It renders the real `App`
// under the app's provider tree (`AppRoot`), over a fixture daemon in place of the WebSocket one. It passes
// `nativeSync={false}` to `AppRoot` and does not reveal the window: the browser platform has no native menu or window.
// It leaves out the syncs to the shell too (the keep-awake icon one, and `connectShellSettings` for the background and menu bar
// icon choices): a scenario that offers `backgroundRun` or `menuBarIcon` gets a capability whose calls do nothing
// (`galleryPlatform`), so there is no shell to push the choice to. Its stored preferences are set from the URL
// before the app loads (`prepareStorage`); the choices the shell holds are set on their preference objects (`shellSettings`).
import React, { useEffect } from "react";
import ReactDOM from "react-dom/client";

import { observeLazyReadiness } from "../components/hooks/lazyReadiness";
import { isLanguage, type Language } from "../i18n/languages";
import { galleryFailure } from "../fixtures/banner";
import type { ShellSystemChange, ShellSystemChangeResult } from "../platform";
import { readHost } from "../platform/host";
import { LinkForwardError } from "../platform/linkForwardError";
import { SCENARIOS } from "./fixtures";
import { installFakeTerminal } from "../fixtures/fakeTerminal";
import { galleryAppMenu } from "./appMenu";
import { holdPrimaryModifier } from "./heldModifier";
import { afterFrames, applyDocumentTheme } from "../fixtures/pageFrame";
import { createUi, settleMotion, stuckExit } from "../fixtures/interact";
import { prepareStorage } from "./prepare";
import { looseRequestSender } from "../requests";
import type { ClientRequestBody } from "../protocol";
import { LIVE_CHANGE_EVENT } from "./scenario";
import { fixtureRemoteDaemons } from "./remoteDaemonsFixture";
import { fixtureRemoteFiles, fixtureRemoteForwards } from "./remoteShellFixture";
import { fixtureUpdater } from "./updaterFixture";

const lazyReadiness = observeLazyReadiness();
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
const heldParam = params.get("held") ?? (scenario?.held ? "1" : "0");
const heldModifier = heldParam === "1" ? holdPrimaryModifier(scenario?.host ?? readHost()) : undefined;
// A scenario that asks for `"from-start"` has the modifier down before its steps, which then find the quick forms by
// their quick names (or either name, where the viewer's Released leaves it up): what opens with the modifier (a menu
// locks its form as it opens) cannot be opened with it afterwards. Any other is pressed after the steps, which find the
// "+" buttons by their plain names.
const heldFromStart = heldModifier !== undefined && scenario?.held === "from-start";

// The app's modules are imported dynamically, after `prepareStorage` above: they read stored preferences (the
// language among them) as they load, so the scenario's own choices have to be in storage first. The converse binds
// everything this file imports statically, `./fixtures`, every scenario file and whatever those import included: none
// of it may read a stored preference at module load, or the scenario opens in the previous one's language (English
// under `lang=zh-CN`, say).
const [{ App }, { AppRoot }, { Toasts }, { TrayFlyout }, { selectPlatform }, startup, { useUpdates }] = await Promise.all([
  import("../App"),
  import("../AppRoot"),
  import("../shell/Toasts"),
  import("../shell/TrayFlyout"),
  import("../platform"),
  import("../StartupScreen"),
  import("../updates/UpdatesProvider"),
]);
// After react-aria's own listeners are registered, as in `main.tsx` (see `focusGuard.ts`).
await import("../input/focusGuard");
await import("../style.css");
const { createFixtureDaemon } = await import("../fixtures/fixtureDaemon");
const { installReportPageFixture } = await import("./reportPageFixture");
const { runInBackground } = await import("../lifecycle/runInBackground");
const { showMenuBarIcon } = await import("../lifecycle/showMenuBarIcon");

const { DaemonFailedScreen, NoAddressMessage, StartupScreen } = startup;

/** The model of the notification-area flyout for a scenario that shows it, built by the app's own menu builder. */
const trayFlyoutModel = scenario?.trayFlyout ? await (await import("./trayFlyoutFixture")).trayFlyoutModel(scenario.trayFlyout, language, theme) : undefined;

/** What a file the gallery's open dialog hands back holds unless the scenario gives one: a document
 * of the right format with nothing in it. */
const EMPTY_DOCUMENT = JSON.stringify({ format: "octoboard-configuration", version: 1, consoles: [] });

/** The window's quit handler once the app has registered it, for a scenario that asks to quit. */
let requestQuit: (() => void) | undefined;
/** The window's handler of the daemon exiting by itself once the app has registered it, for a scenario that says so. */
let daemonExited: ((detail: string) => void) | undefined;

/** The browser platform, with what the desktop app alone offers added so it can be driven in the gallery:
 * `windowChrome` while the URL says `chrome=1` (the top bar's drag region, and on a macOS host the window's own Back
 * and Forward keys) and, with the scenario's `desktopApp`, the desktop platform kind (which the app's shortcuts follow:
 * the console-session switch's Ctrl+Tab on any system, Back and Forward on macOS) and native menu bar (which no menu is
 * drawn for, and which a scenario's `host` without one, as the Windows app's has none, leaves out), the macOS privacy
 * grants stubbed from the scenario's `macosPermissions` (its System Settings pane refused when it gives a
 * `macosSettingsOpenError`), there being no other way to look at those rows than to really change a system setting, a
 * link opener that refuses with the error of a scenario's `externalLinkError` (otherwise the browser's own opener
 * stands, which opens a new tab), `backgroundRun`, `menuBarIcon` and `launchAtLogin` for a scenario that says so (the
 * General switches; none of them reaches a shell, and the login entry is a stub that keeps the last choice, its list of
 * login items refused when the scenario gives a `launchAtLoginOpenError`), the command-line tool for one that gives
 * `commandLine` (a stub that keeps where it stands, its install refused or never finishing as the scenario says), the shell's
 * list of what its features have in effect and its clearing for one that gives `shellSystemChanges` (a stub that lists what was
 * left and answers the clearing as the scenario says, or refuses it unanswered), the desktop shell's `notifications` for one
 * that says `desktopNotifications` (nothing is shown or opened; the link to the system's pane only where the scenario's
 * `host` has one), the clipboard taken away for a scenario that says so, and the configuration-file capability stubbed
 * from the scenario's `configuration` (absent where it says the platform has none) so that no download or file picker
 * is raised from a capture, and the exit capability for a `desktopApp` and for a scenario that says `quitRequested`,
 * whose quit request is raised once its steps have run and whose confirmation ends nothing (or fails with the text of
 * its `exitFailure`), the daemon exiting by itself for a scenario that says `daemonExited`, and the updater for a
 * scenario that has one (`updater`), with that same exit capability, since a restart goes through it, the remote
 * daemons list for a scenario that gives `remoteDaemons`, Remote Login for one that gives `remoteLogin`, and the
 * flyout's own capability for one that gives `trayFlyout`; for a remote window, the ports its shell forwards
 * (`remoteForwards`), the copy of a remote file it stores and opens (`remoteFiles`) and a link opener that fails as the
 * shell's forwarding does (`forwardFailure`). Nothing else native; `chrome=1` alone keeps the browser
 * platform kind. */
function galleryPlatform(): ReturnType<typeof selectPlatform> {
  const platform = selectPlatform();
  const host = scenario?.host ?? platform.host;
  const appMenu = galleryAppMenu(scenario?.desktopApp, scenario?.host);
  const grants = scenario?.macosPermissions;
  const configuration = scenario?.configuration;
  const externalLinkError = scenario?.externalLinkError;
  let clipboardText = typeof scenario?.clipboard === "object" ? scenario.clipboard.text : "";
  // The system's login entry, which the switch changes and a read finds as it was left.
  const launchAtLogin = scenario?.launchAtLogin === undefined ? undefined : { status: scenario.launchAtLogin };
  // The command-line tool's link, which an install makes and a remove takes away.
  const commandLine = scenario?.commandLine && { state: scenario.commandLine.state };
  // What the shell has in effect, which a clearing takes off the list except what it could not clear.
  const systemChanges = scenario?.shellSystemChanges && { listed: [...scenario.shellSystemChanges.changes] };
  const sameChange = (a: ShellSystemChange, b: ShellSystemChange) => JSON.stringify(a) === JSON.stringify(b);
  return {
    ...platform,
    kind: scenario?.desktopApp ? "tauri" : platform.kind,
    host,
    ...((params.get("chrome") === "1" || scenario?.desktopApp) && host.chrome !== "native" && { windowChrome: { leftInset: () => 0, subscribe: () => () => {} } }),
    ...(appMenu && { appMenu }),
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
    ...(scenario?.forwardFailure !== undefined && {
      links: { open: async (url: string) => { throw new LinkForwardError(scenario.forwardFailure!, Number(new URL(url).port)); } },
    }),
    ...(scenario?.remoteDaemons && { remoteDaemons: fixtureRemoteDaemons(scenario.remoteDaemons) }),
    ...(scenario?.remoteForwards && { remoteForwards: fixtureRemoteForwards(scenario.remoteForwards) }),
    ...(scenario?.remoteFiles && { remoteFiles: fixtureRemoteFiles() }),
    ...(scenario?.remoteLogin && {
      remoteLogin: {
        status: async () => (scenario.remoteLogin === "unknown" ? undefined : scenario.remoteLogin === "on"),
        openSystemSettings: async () => {},
      },
    }),
    ...(trayFlyoutModel && {
      // The shell's side of the flyout: the model it built, and nothing to size, choose or close (a choice refused for a
      // scenario that says `chooseFails`).
      trayFlyout: {
        model: async () => trayFlyoutModel,
        onModel: () => () => {},
        onOpened: () => () => {},
        setSize: async () => {},
        choose: async () => {
          if (scenario?.trayFlyout?.chooseFails) throw new Error("gallery: the shell did not take the choice");
        },
        close: async () => {},
      },
    }),
    ...(scenario?.updater && { updater: fixtureUpdater(scenario.updater) }),
    ...((scenario?.desktopApp || scenario?.quitRequested || scenario?.updater || scenario?.daemonExited !== undefined || scenario?.exitFailure !== undefined) && {
      exit: {
        register: ({ onQuitRequested, onDaemonExited }) => {
          requestQuit = onQuitRequested;
          daemonExited = onDaemonExited;
          return () => { requestQuit = undefined; daemonExited = undefined; };
        },
        heartbeat: async () => {},
        beginQuit: async () => {},
        confirmQuit: async () => {
          if (scenario?.exitFailure !== undefined) throw new Error(scenario.exitFailure);
        },
      },
    }),
    ...(scenario?.backgroundRun && { backgroundRun: { set: async () => {} } }),
    ...(scenario?.menuBarIcon && {
      statusItem: { setVisible: async () => {}, setMenu: async () => {}, onSessionChosen: () => () => {}, onOpened: () => () => {} },
    }),
    ...(launchAtLogin && {
      launchAtLogin: {
        status: async () => launchAtLogin.status,
        set: async (enabled: boolean) => (launchAtLogin.status = enabled ? "enabled" : "disabled"),
        openSystemSettings: async () => {
          if (scenario?.launchAtLoginOpenError !== undefined) throw new Error(scenario.launchAtLoginOpenError);
        },
      },
    }),
    ...(commandLine && {
      commandLine: {
        status: async () => {
          if (commandLine.state === "unreadable") throw new Error("gallery: the shell cannot read the link");
          return commandLine.state;
        },
        install: async () => {
          const behaviour = scenario?.commandLine?.install;
          if (behaviour === "pending") return new Promise<never>(() => {});
          if (behaviour) throw behaviour.error;
          return (commandLine.state = { state: "installed", path: "/usr/local/bin/octoboardd", pathNote: false });
        },
        remove: async () => (commandLine.state = { state: "notInstalled" }),
      },
    }),
    ...(systemChanges && {
      systemChanges: {
        list: async () => ({ changes: [...systemChanges.listed], failed: [...(scenario?.shellSystemChanges?.listFailed ?? [])] }),
        clearAll: async () => {
          const unanswered = scenario?.shellSystemChanges?.clearUnanswered;
          if (unanswered !== undefined) throw new Error(unanswered);
          const results: ShellSystemChangeResult[] = scenario?.shellSystemChanges?.clear ?? systemChanges.listed.map((change) => ({ change, outcome: { result: "cleared" } as const }));
          systemChanges.listed = systemChanges.listed.filter((change) => !results.some((result) => result.outcome.result === "cleared" && sameChange(result.change, change)));
          return { results, failed: [...(scenario?.shellSystemChanges?.clearFailed ?? [])] };
        },
      },
    }),
    ...(scenario?.desktopNotifications && {
      notifications: {
        ensurePermission: async () => true,
        notify: async () => {},
        ...(host.notificationSettings === "system-pane" && { openSystemSettings: async () => {} }),
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

// What the shell would hand the window at startup. Nothing is connected to a shell here, so a value set stays in memory.
if (scenario?.shellSettings?.runInBackground !== undefined) runInBackground.set(scenario.shellSettings.runInBackground);
if (scenario?.shellSettings?.showMenuBarIcon !== undefined) showMenuBarIcon.set(scenario.shellSettings.showMenuBarIcon);
// A viewer scenario has one too: the viewer's header copies a path through the daemon's notifications.
const fixtureDaemon = scenario && !scenario.startup && !scenario.trayFlyout ? createFixtureDaemon(scenario) : undefined;
/** How long a read of a file is held back: the real daemon reads on a blocking thread and answers over a socket, so its
 * reply never lands in the task that asked, while the fixture's resolves at once. A viewer that a click on a terminal link
 * opens asks for its file from an effect, and the render that commits what it asked for follows that task: a reply that
 * beat it would be taken for one to a file no longer shown, and the viewer would stay loading. */
const FILE_READ_LATENCY_MS = 10;
const daemon = fixtureDaemon && {
  ...fixtureDaemon,
  request: looseRequestSender(async (body: ClientRequestBody) => {
    if (body.type === "read_project_file" || body.type === "read_path_link") await new Promise((resolve) => setTimeout(resolve, FILE_READ_LATENCY_MS));
    return fixtureDaemon.request(body);
  }),
};
if (daemon && scenario) installReportPageFixture(document, daemon, scenario.id);
const { ViewerGallery } = scenario?.viewer ? await import("./viewerGallery") : { ViewerGallery: undefined };
const viewerSubjects = scenario?.viewer ? await scenario.viewer.subjects(language) : undefined;
const { CodeGallery } = scenario?.codePalette ? await import("./codeGallery") : { CodeGallery: undefined };
const { LicensesGallery } = scenario?.licensesDialog ? await import("./licensesGallery") : { LicensesGallery: undefined };
const { SemanticColoursGallery } = scenario?.semanticColours ? await import("./semanticColoursGallery") : { SemanticColoursGallery: undefined };
const { ButtonsGallery } = scenario?.pendingButtons ? await import("./buttonsGallery") : { ButtonsGallery: undefined };

function screen(): React.ReactElement {
  if (!scenario) return <p className="p-4">Unknown scenario “{params.get("scenario")}”.</p>;
  if (scenario.trayFlyout) return <TrayFlyout />;
  if (CodeGallery) return <CodeGallery />;
  if (LicensesGallery && scenario.licensesDialog) return <LicensesGallery state={scenario.licensesDialog} />;
  if (SemanticColoursGallery) return <SemanticColoursGallery />;
  if (ButtonsGallery) return <ButtonsGallery />;
  switch (scenario.startup?.kind) {
    case "noAddress":
      return <StartupScreen message={<NoAddressMessage />} />;
    case "daemonFailed":
      return <DaemonFailedScreen error={scenario.startup.error} cause={scenario.startup.cause} />;
    case "crash":
      return <Crash stack={scenario.startup.stack} />;
  }
  if (viewerSubjects && ViewerGallery)
    return (
      <>
        <ViewerGallery subjects={viewerSubjects} loadDelay={scenario.viewer?.loadDelay} quote={scenario.viewer?.quote} />
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

/** Makes the page a remote daemon's window as the shell opens one: `?remote=<name>` in its address, which the app reads
 * only inside the shell's webview (`remoteDaemonName` asks `inTauri`, the IPC bridge the shell injects). The marker is
 * put on the window only now, after the platform adapter has been chosen: choosing it under the marker would build the
 * shell's own adapter, whose capabilities need the bridge, in place of the browser's the gallery adds to. What reads
 * the marker and the name does so as it renders, so the title bar names the daemon and the application's own features
 * are absent. The daemon is not this machine's (`isLocal`), so what acts on its files is absent too, and the window's
 * client retries until it connects (`retriesUntilConnected`), as the shell's remote window's does. */
function enterRemoteWindow(name: string): void {
  const address = new URL(window.location.href);
  address.searchParams.set("remote", name);
  window.history.replaceState(null, "", address);
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
}
const platformOfWindow = galleryPlatform();
if (scenario?.remoteWindow) enterRemoteWindow(scenario.remoteWindow.name);

// No automatic update check follows on its own, which would move a settled view 30 seconds on
// (`firstCheckDelayMs={null}`): a scenario that wants one says `checked`.
ReactDOM.createRoot(document.getElementById("root")!, {
  onCaughtError: (error, info) => {
    lazyReadiness.caught(error);
    console.error(error, info.componentStack);
  },
}).render(
  <React.StrictMode>
    <AppRoot
      platform={platformOfWindow}
      daemon={daemon && scenario?.remoteWindow ? { ...daemon, isLocal: false, retriesUntilConnected: true } : daemon}
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
// `input=mouse` has the steps press as a mouse does, which draws no keyboard focus ring: the docs' screenshots use it.
const ui = createUi(document, language, daemon, scenario?.host ?? readHost(), { input: params.get("input") === "mouse" ? "mouse" : "virtual" });
// Listening before the steps, so the buttons work on a page whose steps failed; a change waits for them.
const live = new LiveChanges(ui);
document.addEventListener(LIVE_CHANGE_EVENT, (event) => void live.apply(String((event as CustomEvent<unknown>).detail)));
try {
  await ui.wait(300);
  if (heldFromStart) heldModifier.press();
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
  if (!heldFromStart) heldModifier?.press();
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
// A lazy fallback is observed through its commit lifetime, including children revealed by a resolved parent.
// Intentional fixture requests stay outside this barrier, so a busy dialog is still a settled gallery view.
try {
  await lazyReadiness.settle(async () => {
    await afterFrames(200);
    await settleMotion(document, 1000);
  }, 15_000);
} catch (error) {
  galleryFailure(`The scenario's lazy content is not ready: ${(error as Error).message}`, error);
}
document.documentElement.dataset.galleryReady = "true";
