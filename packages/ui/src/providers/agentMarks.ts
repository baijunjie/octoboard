import { antigravity } from "./antigravity";
import { claude } from "./claude";
import { codex } from "./codex";
import { copilot } from "./copilot";
import { grok } from "./grok";
import { opencode } from "./opencode";
import { pi } from "./pi";
import { qwen } from "./qwen";

export { markArt } from "./markArt";

/** Each built-in provider's mark, by provider id: the entry `@octoboard/ui/agent-marks` exposes to the website, which
 * draws the marks without the UI's registry. */
export const AGENT_MARKS = {
  claude: claude.mark,
  codex: codex.mark,
  grok: grok.mark,
  antigravity: antigravity.mark,
  copilot: copilot.mark,
  opencode: opencode.mark,
  qwen: qwen.mark,
  pi: pi.mark,
};
