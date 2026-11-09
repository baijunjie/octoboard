import { looseRequestSender } from "../requests";
import { CodedError } from "../errors/codedError";
import { daemonMessage } from "../errors/daemonMessage";
import type { Translate } from "../i18n/catalog";
import { currentLanguage, t } from "../i18n/language";
import type { Event, ClientRequestBody } from "../protocol";
import { createImportProgressListeners, type Daemon, type WindowNotification } from "../store";
import { broadcastNotification, errorNotification } from "../store/notifications";
import type { Fixture, NotificationFixture } from "./fixture";
import { terminalFixtureUrl } from "./fakeTerminal";
import { createConfigurationReplies } from "./fixtureConfiguration";
import { createGitReplies, GIT_REQUESTS } from "./fixtureGit";
import { createAgentCliReplies } from "./fixtureAgentClis";
import { createAccountGuards, createAccountReplies } from "./fixtureAccounts";
import { createConsoleReplies } from "./fixtureConsoles";
import { createFixtureContext, mergeHandlers, neverReplied } from "./fixtureContext";
import { createFileReplies, FILE_REQUESTS } from "./fixtureFiles";
import { createLifecycleReplies } from "./fixtureLifecycle";
import { createProcessLedger, createProcessReplies } from "./fixtureProcesses";
import { createProjectReplies } from "./fixtureProjects";
import { createSessionReplies } from "./fixtureSessions";
import { createSettingsReplies, keepProviderSettingsInEffect } from "./fixtureSettings";

/** How long after the app subscribes to notifications the scenario's own are raised, for the stack to be
 * on screen to receive them. */
export const NOTIFICATION_DELAY_MS = 300;

/** The daemon the gallery runs a scenario over: `scenarioNotificationsRaised` settles once the scenario's own
 * notifications have been handed to the stack, which the page waits for before it calls itself ready. It never settles
 * if nothing subscribes to notifications. */
export type FixtureDaemon = Daemon & { readonly scenarioNotificationsRaised: Promise<void> };

/** The requests that read what a project's folder holds, which a page showing several repositories behind one daemon
 * has to send to the repository the project belongs to: the Git ones (`fixtureGit.ts`) and the file ones
 * (`fixtureFiles.ts`). */
export const BROWSE_REQUESTS: ReadonlySet<string> = new Set<string>([...GIT_REQUESTS, ...FILE_REQUESTS]);

/**
 * A `Daemon` with no connection behind it, for the gallery: the store holds the scenario's state as
 * it is, requests are answered from the scenario (`list_dir`, `list_pages`, the browse requests) or
 * accepted and ignored (settings and trusted folders are updated, a console is added or saved, and
 * session-file inspections are answered explicitly and deleting sessions removes them), and the scenario's
 * notifications are raised once the app is listening.
 *
 * A project pane's Git-mode requests are answered by `fixtureGit.ts` instead, and the export and import
 * of configuration by `fixtureConfiguration.ts`; each is asked before anything here and says with
 * `undefined` that a request is not one of its own.
 */
export function createFixtureDaemon(scenario: Fixture): FixtureDaemon {
  const context = createFixtureContext(scenario);
  const { store, git, restored, fail, providerSettingChoices } = context;
  keepProviderSettingsInEffect(context);
  const notificationListeners = new Set<(notification: WindowNotification) => void>();
  const emitNotification = (notification: WindowNotification) => notificationListeners.forEach((listener) => listener(notification));
  let notificationsRaised = false;
  let markNotificationsRaised!: () => void;
  const scenarioNotificationsRaised = new Promise<void>((resolve) => (markNotificationsRaised = resolve));

  /** The notification a fixture stands for, made by the code the real route uses. */
  const scenarioNotification = (fixture: NotificationFixture): WindowNotification => {
    const worded = (message: string | ((t: Translate) => string)) => (typeof message === "function" ? message(t) : message);
    switch (fixture.kind) {
      case "broadcast":
        return broadcastNotification(fixture.event, store.getState(), currentLanguage());
      case "notice":
        return { kind: "notice", message: worded(fixture.message) };
      case "raised": {
        // The request `raiseError` builds, which `Toasts` turns into the same alert. The failure is as `request()` rejects
        // it: its message is worded already, which the alert's body and its dedup read.
        const fallback = worded(fixture.message);
        const { code, params = {} } = fixture;
        const error = code
          ? new CodedError(daemonMessage(currentLanguage(), code, params, fallback, store.getState()), code, params)
          : new Error(fallback);
        return errorNotification(error, { title: worded(fixture.title), session: fixture.session, recovery: fixture.recovery });
      }
    }
  };

  const raiseScenarioNotifications = () => {
    if (notificationsRaised) return;
    notificationsRaised = true;
    // Settled even when a fixture throws, so the page reports that error and not a missing subscriber.
    try {
      for (const fixture of scenario.notifications ?? []) emitNotification(scenarioNotification(fixture));
    } finally {
      markNotificationsRaised();
    }
  };

  const gitReply = createGitReplies(git, fail);
  const importProgress = createImportProgressListeners();
  const configurationReply = createConfigurationReplies(scenario.configuration, store.getState, fail, () => providerSettingChoices.current, importProgress.deliver, (tag) => store.setState((current) => ({ settings: { ...current.settings, interface_language: tag } })));
  const guards = createAccountGuards(context);
  const ledger = createProcessLedger(context);
  const replies = mergeHandlers(
    createSessionReplies(context, guards, ledger),
    createConsoleReplies(context),
    createAccountReplies(context),
    createAgentCliReplies(context),
    createSettingsReplies(context),
    createProjectReplies(context, ledger),
    createFileReplies(context),
    createProcessReplies(context, ledger),
    createLifecycleReplies(context),
  );

  return {
    store,
    scenarioNotificationsRaised,
    request: looseRequestSender(async (body: ClientRequestBody): Promise<Event> => {
      if (body.type === "get_project_source") {
        const result = scenario.projectSources?.[body.project];
        if (result === "pending") return neverReplied();
        if (result) return fail(result);
      }
      if (body.type === "list_project_branches") {
        const result = scenario.projectBranches?.[body.project];
        if (result === "pending") return neverReplied();
        if (result) return fail(result);
      }
      const fromGit = ("project" in body && typeof body.project === "string" ? restored.git.get(body.project) ?? gitReply : gitReply)(body);
      if (fromGit !== undefined) return fromGit;
      const fromConfiguration = configurationReply(body);
      if (fromConfiguration !== undefined) return fromConfiguration;
      const reply = replies[body.type] as ((body: ClientRequestBody) => Event | Promise<Event>) | undefined;
      return reply ? reply(body) : { type: "ack", id: null };
    }),
    showNotice: (message, content) => emitNotification({ kind: "notice", message, content }),
    onNotification: (listener) => {
      notificationListeners.add(listener);
      // StrictMode subscribes, unsubscribes and subscribes again; by the time this fires only the
      // last subscription is left.
      setTimeout(raiseScenarioNotifications, NOTIFICATION_DELAY_MS);
      return () => void notificationListeners.delete(listener);
    },
    // What an import reports is the scenario's to give (`ConfigurationFixture.progress`); the fixture delivers it to the
    // listener of the import the request named, before the reply. The real daemon may deliver an event after the reply
    // (see `apps/daemon/PROTOCOL.md`, "Configuration import"); a scenario does not rely on either order.
    onImportProgress: importProgress.subscribe,
    dismissTrustPrompt: (session) =>
      store.setState((state) => ({ trustPrompts: state.trustPrompts.filter((p) => p.session !== session) })),
    reconnect: () => {},
    // A shell terminal follows `shellTerminal` rather than `terminal`, which is the sessions'; one
    // whose shell has exited shows its final screen and closes, as the daemon's socket does.
    terminalUrl: (id) => {
      const shell = store.getState().shellTerminals.get(id);
      if (!shell) return terminalFixtureUrl(scenario.terminal ?? "output", id);
      // A shell this window started draws a fresh prompt in the folder it started in.
      return terminalFixtureUrl(shell.state === "exited" ? "exited" : scenario.shellTerminal ?? "shell", id, context.startedShellTerminals.has(id) ? shell.cwd : undefined);
    },
    terminalReconnectDelay: scenario.terminalRetriesImmediate ? () => 0 : undefined,
  };
}
