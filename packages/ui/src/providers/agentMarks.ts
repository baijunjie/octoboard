import { claude } from "./claude";
import { codex } from "./codex";
import { grok } from "./grok";

/** Each built-in provider's mark, by provider id: the entry `@octoboard/ui/agent-marks` exposes to the website, which
 * draws the marks without the UI's registry. */
export const AGENT_MARKS = {
  claude: claude.mark,
  codex: codex.mark,
  grok: grok.mark,
};
