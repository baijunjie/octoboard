import { shownModel } from "../agents/agentLaunchSettings";
import { providerModelDisplay } from "../agents/providers";
import { consoleProjectLaunch, inheritedSessionLaunch, launchWithOverrides } from "../agents/agents";
import type { Translate } from "../i18n/catalog";
import { isolate } from "../i18n/isolate";
import type { Console, Launch, ModelDisplay, Project, ProviderInfo, Session } from "../protocol";

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
  if (!owner) return { model: null, effort: null };
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
  };
}

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
