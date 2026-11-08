// The context the fixture daemon's domain modules share: the state they are built over, the shape of the replies they
// give, and the small helpers more than one of them needs.
import type { ReplyOf } from "../requests";
import { CodedError } from "../errors/codedError";
import { daemonMessage } from "../errors/daemonMessage";
import { currentLanguage } from "../i18n/language";
import { type Event, type Launch, type LaunchChange, type PerAgent, type ClientRequestBody, type Session, type Settings } from "../protocol";
import { createStateStore } from "../store";
import type { Fixture } from "./fixture";
import { createGitReplies } from "./fixtureGit";
import { defaultAgentCliStatuses } from "./fixtureAgentClis";
import { FIXTURE_AGENTS, FIXTURE_PROVIDERS, availabilityOf } from "./fixtureProviders";
import type { FixtureError, FixtureFiles } from "./sampleFiles";
import { SAMPLE_GIT } from "./sampleGit";
import type { FixtureGit } from "./changeFixtures";

export const HOME = "/Users/dev";

/** A path as the daemon compares one: `~` and `~/` expanded against the home directory, `.` and
 * `..` and repeated or trailing slashes resolved lexically, symlinks not followed. */
export function normalizedPath(path: string): string {
  const rest = path.startsWith("~/") ? path.slice(2) : undefined;
  const expanded = path === "~" ? HOME : rest === undefined ? path : rest.startsWith("/") ? rest : `${HOME}/${rest}`;
  const parts: string[] = [];
  for (const part of expanded.split("/")) {
    if (part === "..") parts.pop();
    else if (part !== "" && part !== ".") parts.push(part);
  }
  return `${expanded.startsWith("/") ? "/" : ""}${parts.join("/")}`;
}

/** A text as the daemon stores one that may be left blank: trimmed, and none when nothing is left
 * (`trimmed_or_none` of `launch_settings.rs`). */
export const trimmedOrNull = (text: string | null | undefined): string | null => text?.trim() || null;

/** One per-agent group after a request's `Partial<PerAgent<…>>`: an agent the request leaves out keeps its value, and
 * `change` says what the request's value for one agent makes of the current one. */
export function changedPerAgent<T, C>(current: PerAgent<T>, changes: Partial<PerAgent<C>> | undefined, change: (value: T, change: C) => T): PerAgent<T> {
  return Object.fromEntries(FIXTURE_AGENTS.map((agent) => {
    const requested = changes?.[agent];
    return [agent, requested === undefined ? current[agent] : change(current[agent], requested)];
  })) as PerAgent<T>;
}

/** One agent's launch settings after a request's change: a field the request leaves out keeps its value, and one it names
 * is trimmed, `null` or blank clearing it. */
export function changedLaunch(current: Launch, change: LaunchChange): Launch {
  return {
    model: change.model === undefined ? current.model : trimmedOrNull(change.model),
    effort: change.effort === undefined ? current.effort : trimmedOrNull(change.effort),
  };
}

export function normalizeTags(tags: string[]): string[] {
  const seen = new Set<string>();
  return tags.map((tag) => tag.trim()).filter((tag) => {
    if (!tag || seen.has(tag.toLowerCase())) return false;
    seen.add(tag.toLowerCase());
    return true;
  });
}

/** The store a fixture daemon holds. */
export type FixtureStore = ReturnType<typeof createStateStore>;

/** The reply of a request with nothing to return. */
export type Ack = Extract<Event, { type: "ack" }>;

/** A reply that never comes, for a scenario that holds a request pending. */
export const neverReplied = (): Promise<never> => new Promise<never>(() => {});

/** The reply to one kind of request: a value, or a promise of one that may never settle. A refusal is thrown (`fail`). */
export type Handler<K extends ClientRequestBody["type"]> = (body: Extract<ClientRequestBody, { type: K }>) => ReplyOf<K> | Promise<ReplyOf<K>>;

/** What a domain's module answers: a reply for each kind of request it knows. A request with no entry anywhere is
 * acknowledged. */
export type Handlers = { [K in ClientRequestBody["type"]]?: Handler<K> };

/** What the domain modules of the fixture daemon share: the scenario, the store the window reads, and the state more
 * than one domain reads or writes. */
export interface FixtureContext {
  scenario: Fixture;
  store: FixtureStore;
  /** The repository the scenario's projects are read from: the sample one unless the scenario gives another or none. */
  git: FixtureGit | null;
  /** Throws the daemon error `error` stands for, worded as the store words one. */
  fail: (error: FixtureError) => never;
  refuseUnknownProject: (project: string) => void;
  refuseLocked: (sessions: Iterable<Session>) => void;
  /** The scenario's `settings.provider_settings` read as the user's explicit choices, as the daemon stores them (what
   * the window is told is the value in effect, `effectiveProviderSettings`); `set_provider_setting` writes them and
   * configuration export reads them. */
  providerSettingChoices: { current: Settings["provider_settings"] };
  /** What a project's files and repository are after a branch switch or a restore, in place of the scenario's. */
  restored: { files: Map<string, FixtureFiles>; git: Map<string, ReturnType<typeof createGitReplies>> };
  /** The shell terminals this window started, which draw a fresh prompt instead of the sample of a used shell. */
  startedShellTerminals: Set<string>;
}

export function createFixtureContext(scenario: Fixture): FixtureContext {
  const agentAvailability = scenario.state?.agentAvailability ?? availabilityOf(FIXTURE_PROVIDERS);
  // Every installed agent checked and current, unless the scenario holds statuses of its own (an empty map: none checked).
  const store = createStateStore({ providers: FIXTURE_PROVIDERS, agentAvailability, agentCliStatuses: defaultAgentCliStatuses(agentAvailability), ...scenario.state });
  const fail = ({ code, params, message }: FixtureError): never => {
    throw new CodedError(daemonMessage(currentLanguage(), code, params, message, store.getState()), code, params);
  };
  return {
    scenario,
    store,
    git: scenario.git === undefined ? SAMPLE_GIT : scenario.git,
    fail,
    refuseUnknownProject: (project) => {
      if (!store.getState().projects.has(project)) fail({ code: "unknown_project", params: { project }, message: `unknown project ${project}` });
    },
    refuseLocked: (sessions) => {
      const locked = [...sessions].find((session) => session.locked);
      if (locked) fail({ code: "session_locked", params: { session: locked.id }, message: "This session is locked" });
    },
    providerSettingChoices: { current: scenario.state?.settings?.provider_settings ?? {} },
    restored: { files: new Map(), git: new Map() },
    startedShellTerminals: new Set(),
  };
}

/** The union of the domain modules' replies. A kind of request is answered by one module only: a second answer for it
 * is a mistake, not an override. */
export function mergeHandlers(...groups: Handlers[]): Handlers {
  const merged: Record<string, unknown> = {};
  for (const group of groups) {
    for (const [type, handler] of Object.entries(group)) {
      if (type in merged) throw new Error(`fixtureDaemon: ${type} is answered by two modules`);
      merged[type] = handler;
    }
  }
  return merged as Handlers;
}
