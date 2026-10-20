import { consoleProjectLaunch, inheritedSessionLaunch, type LaunchPair, launchWithOverrides } from "./agents";
import type { Translate } from "./i18n/catalog";
import { isolate } from "./i18n/isolate";
import type { Agent, Console, Project, Session } from "./protocol";

const CLAUDE_ALIASES = new Set(["opus", "sonnet", "haiku"]);
/** A Claude model id: the family, then the version parts, then optionally a release date. */
const CLAUDE_ID = /^claude-([a-z]+)((?:-\d{1,2})+)(?:-\d{8})?$/;

const capitalized = (word: string): string => word.charAt(0).toUpperCase() + word.slice(1);

/** A short name for a model id: for Claude, `claude-opus-5-5` reads "Opus 5.5" and the alias
 * `sonnet` reads "Sonnet"; the `best` alias is named in the catalogue, as the launch pickers name
 * it. Claude is the only agent whose ids are long enough to need it; any other agent's id, and a
 * Claude one of a shape not known, is shown as it is. */
export function modelDisplayName(t: Translate, agent: Agent, model: string): string {
  if (agent !== "claude") return model;
  if (model === "best") return t("dialog.agentSettings.modelBest");
  if (CLAUDE_ALIASES.has(model)) return capitalized(model);
  const match = CLAUDE_ID.exec(model);
  if (!match) return model;
  return `${capitalized(match[1])} ${match[2].slice(1).replaceAll("-", ".")}`;
}

/** What the session takes when its launch override is absent. A project session whose project is
 * not in the store yet still takes its console's project session pair. */
export function inheritedLaunch(session: Session, consoles: Map<string, Console>, projects: Map<string, Project>): LaunchPair {
  const owner = consoles.get(session.console_id);
  if (!owner) return { model: null, effort: null };
  const project = session.project_id ? projects.get(session.project_id) : undefined;
  if (session.project_id && !project) return consoleProjectLaunch(owner, session.agent);
  return inheritedSessionLaunch(owner, project, session.agent);
}

/** The model and effort a session is shown running with, each from what the daemon observed in the
 * agent's records, else the session's launch override, else `inherited`. The effort the records
 * leave unset is, before those two, the level the daemon found the agent runs the observed model at
 * by itself, since the records show the session running without one. `null` is the agent's own
 * default where nothing says which it is. */
export function sessionModelEffort(session: Session, inherited: LaunchPair): LaunchPair {
  const launched = launchWithOverrides(inherited, session);
  return {
    model: session.observed_model ?? launched.model,
    effort: session.observed_effort ?? session.default_effort ?? launched.effort ?? null,
  };
}

/** "Opus 5.5 · high" for the line beside a session's agent mark. An item not known reads as the
 * default, and a session with neither known says so once. */
export function sessionModelLabel(t: Translate, agent: Agent, launch: LaunchPair): string {
  if (launch.model === null && launch.effort === null) return t("session.modelEffort.default");
  return t("session.modelEffort", {
    model: launch.model === null ? t("session.modelEffort.unknownItem") : isolate(modelDisplayName(t, agent, launch.model)),
    effort: launch.effort === null ? t("session.modelEffort.unknownItem") : isolate(launch.effort),
  });
}
