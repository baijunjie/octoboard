import { readFileSync } from "node:fs";

const aiName = /^(?:claude(?: code)?|codex|chatgpt|(?:github )?copilot|gemini|cursor|aider|grok)$/i;
const aiEmail = /^(?:(?:noreply|claude)@anthropic\.com|(?:noreply|codex)@openai\.com|copilot@github\.com)$/i;
const generatedByAi = /^\s*(?:🤖\s*)?generated (?:with|by)\s+\[?(?:claude|codex|chatgpt|(?:github )?copilot|gemini|cursor|aider|grok)\b/i;

try {
  if (process.argv.length !== 3) throw new Error("Expected the commit message file path.");
  const message = readFileSync(process.argv[2], "utf8");
  const attribution = message.split(/\r?\n/).find((line) => {
    const trailer = line.match(/^\s*(?:co-authored-by|signed-off-by):\s*(.*)$/i);
    if (trailer) {
      const identity = trailer[1].match(/^(.*?)\s*<([^<>]+)>\s*$/);
      return aiName.test((identity?.[1] ?? trailer[1]).trim()) || aiEmail.test(identity?.[2] ?? "");
    }
    return generatedByAi.test(line);
  });
  if (attribution) throw new Error(`AI attribution is not allowed. Remove this line and commit again:\n${attribution}`);
} catch (error) {
  console.error(`commit-msg: ${error.message}`);
  process.exitCode = 1;
}
