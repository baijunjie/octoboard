import type { Agent } from "../protocol";

/** Whether the agent has the Claude compatibility setting: every agent but Claude Code. Takes any string, since a
 * document from a build that knows more agents lists keys this one has no such setting for. The one place that says
 * which agents have it. */
export function hasClaudeCompatibility(agent: string): agent is Exclude<Agent, "claude"> {
  return agent === "codex" || agent === "grok";
}
