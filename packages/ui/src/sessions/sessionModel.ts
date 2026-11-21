import { effortLevels, shownModel, type LaunchOverrides } from "../agents/agentLaunchSettings";
import { providerModelDisplay } from "../agents/providers";
import { consoleProjectLaunch, inheritedSessionLaunch, launchWithOverrides } from "../agents/agents";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { AgentModel, Console, Launch, ModelDisplay, Project, ProviderInfo, Session } from "../protocol";

/** The rest of a model id after its maker's prefix: the family, then the version parts, then optionally a release date. */
const FAMILY_VERSION = /^([a-z]+)((?:-\d{1,2})+)(?:-\d{8})?$/;

/** A model id with a bracketed suffix, as an agent takes a context window (`opus[1m]`). */
const BRACKETED_SUFFIX = /^(.+?)(\[[^\]]+\])$/;

const capitalized = (word: string): string => word.charAt(0).toUpperCase() + word.slice(1);

/** A short name for a model id, by how the provider's `display` rule (served with it) reads one: with `family_version`,
 * `claude-opus-5-5` reads "Opus 5.5" and an alias it lists, `sonnet`, reads "Sonnet". Any other id, and one of a shape
 * not known (a retired alias among them), is shown as it is, as is every id of a provider without a rule. */
export function modelDisplayName(model: string, display: ModelDisplay | undefined): string {
  if (!display) return model;
  // A context-window suffix (`opus[1m]`) stays on the end of the name it qualifies.
  const suffixed = BRACKETED_SUFFIX.exec(model);
  if (suffixed) {
    const base = modelDisplayName(suffixed[1], display);
    return base === suffixed[1] ? model : `${base} ${suffixed[2]}`;
  }
  if (display.aliases.includes(model)) return capitalized(model);
  if (!display.family_version) return model;
  // Only ids of the one maker's spelling when the rule names it, else of whichever maker's prefix the id starts with.
  const prefix = display.id_prefix ?? /^[a-z]+-/.exec(model)?.[0];
  if (prefix === undefined || !model.startsWith(prefix)) return model;
  const match = FAMILY_VERSION.exec(model.slice(prefix.length));
  if (!match) return model;
  return `${capitalized(match[1])} ${match[2].slice(1).replaceAll("-", ".")}`;
}

/** What the session takes when its launch override is absent. A project session whose project is
 * not in the store yet still takes its console's project session pair. */
export function inheritedLaunch(session: Session, consoles: Map<string, Console>, projects: Map<string, Project>, providers: readonly ProviderInfo[]): Launch {
  const owner = consoles.get(session.console_id);
  if (!owner) return { model: null, effort: null, compatibility: {} };
  const project = session.project_id ? projects.get(session.project_id) : undefined;
  if (session.project_id && !project) return consoleProjectLaunch(owner, session.agent);
  return inheritedSessionLaunch(owner, project, session.agent, providerModelDisplay(providers, session.agent));
}

/** The model and effort a session is shown running with (`display` is its agent's model display), each from what the daemon observed in the
 * agent's records, else the session's launch override, else `inherited`. The effort the records
 * leave unset is, before those two, the level the daemon found the agent runs the observed model at
 * by itself, since the records show the session running without one. `null` is the agent's own
 * default where nothing says which it is. */
export function sessionModelEffort(session: Session, inherited: Launch, display: ModelDisplay | undefined): Launch {
  const launched = launchWithOverrides(inherited, session);
  return {
    model: session.observed_model ?? shownModel(display, launched.model),
    effort: session.observed_effort ?? session.default_effort ?? launched.effort ?? null,
    compatibility: { ...inherited.compatibility, ...session.compatibility },
  };
}

/** The pair a session's launch pickers start on: `launch`, what it is launched with, except where the agent's records
 * show it running with something else, so that the pickers name the model the line beside its agent mark shows and
 * the value it is launched with can be picked as a change. The observed model is read as one of `models` (the agent's list): the
 * one whose id it is, or the alias whose version is its short name. It replaces the launch model only when that one,
 * or `defaultModel` where the launch names none, is known to be a different model; an observed model the list does
 * not name, and a launch model it does not name, leave the launch model in place. The observed effort replaces the
 * launch one only where it is a level the effort picker offers for the model shown, so nothing is shown from the
 * records before the list is known; an effort the launch leaves to the agent stays unset, since the observed one may
 * be that very default. The compatibility switches are `launch`'s own. */
export function runningLaunch(session: Session, launch: Launch, models: AgentModel[], defaultModel: string | null, display: ModelDisplay | undefined): Launch {
  const observed = session.observed_model;
  const name = observed === undefined ? undefined : modelDisplayName(observed, display);
  const runs = (model: AgentModel) => model.id === observed || (model.version !== undefined && model.version === name);
  const launched = models.find((model) => model.id === (launch.model ?? defaultModel));
  const running = launched && !runs(launched) ? models.find(runs) : undefined;
  const model = running?.id ?? launch.model;
  const observedEffort = session.observed_effort;
  const offered = observedEffort !== undefined && effortLevels(models, model ?? defaultModel).levels.includes(observedEffort);
  return { ...launch, model, effort: launch.effort !== null && offered ? observedEffort : launch.effort };
}

/** What a confirmed change of a session's model sends: the `LaunchOverrides` to change, and `reapply` where they are to
 * be applied to the running process even if the session already holds them (it runs what its records show, which differs
 * from its launch values). */
export type SessionModelChange = LaunchOverrides & { reapply?: true };

/** The model's short name and the effort as separate values, for the labelled list of a session's
 * agent tooltip. A value not known reads as the default. */
export function sessionModelValues(t: Translate, display: ModelDisplay | undefined, launch: Launch): { model: string; effort: string } {
  return {
    model: launch.model === null ? t("session.modelEffort.unknownItem") : modelDisplayName(launch.model, display),
    effort: launch.effort ?? t("session.modelEffort.unknownItem"),
  };
}

/** "Opus 5.5 · high" for the line beside a session's agent mark. An item not known reads as the
 * default, and a session with neither known says so once. */
export function sessionModelLabel(t: Translate, display: ModelDisplay | undefined, launch: Launch): string {
  if (launch.model === null && launch.effort === null) return t("session.modelEffort.default");
  return t("session.modelEffort", {
    model: launch.model === null ? t("session.modelEffort.unknownItem") : isolate(modelDisplayName(launch.model, display)),
    effort: launch.effort === null ? t("session.modelEffort.unknownItem") : isolate(launch.effort),
  });
}
