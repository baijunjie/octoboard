// Search production core code for provider knowledge; exceptions are exact source lines with reasons.
import { readdirSync, readFileSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const EXCEPTIONS = [
  ...["CLAUDE", "CODEX", "GROK"].map((id) => ({
    file: "apps/daemon/src/adapter/mod.rs",
    line: `(AgentId::${id}, &${id.toLowerCase()}::${id[0]}${id.slice(1).toLowerCase()}Provider),`,
    reason: "The provider adapter registry is the one permitted registration point.",
  })),
];

// Retain positions and newlines so diagnostics name the original source line. Strings are kept:
// their contents are exactly the agent facts this check must inspect.
const TEST_CFG = /#\[cfg\((test|all\([^\]]*\))\)\]/g;
// Only a direct `test` conjunct proves an item absent from production. `any(test, unix)` does not.
export const testOnly = (condition) => {
  if (condition === "test") return true;
  if (!condition.startsWith("all(")) return false;
  let depth = 0;
  let part = "";
  for (const char of `${condition.slice(4, -1)},`) {
    if (char === "," && depth === 0) {
      if (testOnly(part.trim())) return true;
      part = "";
    } else {
      if (char === "(") depth++;
      if (char === ")") depth--;
      part += char;
    }
  }
  return false;
};

export function productionSource(source, rust = false, structureOnly = false, css = false) {
  const tokens = [];
  const pattern = css
    ? /\/\*[\s\S]*?\*\/|"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'/g
    : /\/\/[^\n]*|\/\*[\s\S]*?\*\/|`(?:\\[\s\S]|[^`\\])*`|r(#+)?"[\s\S]*?"\1|"(?:\\[\s\S]|[^"\\])*"|'(?:\\.|[^'\\])'/g;
  let match;
  while ((match = pattern.exec(source))) tokens.push({ start: match.index, end: pattern.lastIndex, comment: match[0].startsWith("//") || match[0].startsWith("/*") });
  const chars = source.split("");
  const blank = (start, end) => { for (let i = start; i < end; i++) if (chars[i] !== "\n") chars[i] = " "; };
  for (const token of tokens) if (token.comment || structureOnly) blank(token.start, token.end);
  if (rust) {
    const visible = productionSource(source, false, true);
    const test = new RegExp(TEST_CFG);
    while ((match = test.exec(visible))) {
      if (!testOnly(match[1])) continue;
      let depth = 0;
      let opened = false;
      let i = test.lastIndex;
      let tokenIndex = 0;
      for (; i < source.length; i++) {
        while (tokenIndex < tokens.length && tokens[tokenIndex].end <= i) tokenIndex++;
        const token = tokens[tokenIndex]?.start <= i ? tokens[tokenIndex] : undefined;
        if (token) { i = token.end - 1; continue; }
        if (source[i] === "{") { depth++; opened = true; }
        if (source[i] === "}" && --depth === 0 && opened) { i++; break; }
        if (source[i] === ";" && !opened) { i++; break; }
      }
      blank(match.index, i);
      test.lastIndex = i;
    }
  }
  return chars.join("");
}

const KNOWLEDGE = /AgentId\s*::\s*(?:CLAUDE|CODEX|GROK)\b|["'`](?:claude(?:-code)?|codex|grok(?:-build)?)\\?["'`]|(?:\bClaude\b|\bGrok\b|\bCodex\b)|\.(?:claude|codex|grok)\b|(?:CLAUDE|AGENTS)\.md|\b(?:CLAUDECODE|(?:CLAUDE|CODEX|GROK)_[A-Z0-9_]+)\b|(?:--permission-mode|--approval-policy|--ask-for-approval|--dangerously-skip-permissions|--dangerously-bypass-approvals-and-sandbox)|(?:codex|claude|grok):\/\/|\b(?:PreToolUse|PostToolUse|UserPromptSubmit|StopCancelled|idle_prompt)\b|\b(?:config\.toml|auth\.json)\b/g;

export function violations(file, source, exceptions = EXCEPTIONS) {
  const production = productionSource(source, file.endsWith(".rs"), false, file.endsWith(".css"));
  const lines = source.split("\n");
  const hitLines = new Set([...production.matchAll(KNOWLEDGE)].map((match) => production.slice(0, match.index).split("\n").length - 1));
  return [...hitLines].flatMap((index) => {
    const line = lines[index].trim();
    if (exceptions.some((exception) => exception.file === file && exception.line === line && exception.reason)) return [];
    return [{ file, line: index + 1, text: line }];
  });
}

export function checkProviderNeutral(root) {
  const failures = [];
  const testModules = new Set();
  const testModule = /#\[cfg\((test|all\([^\]]*\))\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;/g;
  const discover = (directory) => {
    for (const entry of readdirSync(path.join(root, directory), { withFileTypes: true })) {
      const file = `${directory}/${entry.name}`;
      if (entry.isDirectory()) discover(file);
      else if (entry.isFile() && file.endsWith(".rs")) {
        const source = productionSource(readFileSync(path.join(root, file), "utf8"), false, true);
        for (const match of source.matchAll(testModule)) {
          if (!testOnly(match[1])) continue;
          const parent = /\/(?:mod|main|lib)\.rs$/.test(file) ? path.dirname(file) : file.slice(0, -3);
          testModules.add(`${parent}/${match[2]}.rs`);
          testModules.add(`${parent}/${match[2]}`);
        }
      }
    }
  };
  discover("apps/daemon/src");
  for (const base of ["apps/daemon/src", "packages/ui/src"]) {
    const walk = (relative) => {
      for (const entry of readdirSync(path.join(root, relative), { withFileTypes: true })) {
        const file = `${relative}/${entry.name}`;
        if (testModules.has(file)) continue;
        if (entry.isDirectory()) {
          if (file === `${base}/providers` || ["packages/ui/src/gallery", "packages/ui/src/fixtures", "packages/ui/src/website-preview"].includes(file)) continue;
          if (["tests", "test", "test_support", "test-fixtures", "fixtures"].includes(entry.name)) continue;
          if (relative === "apps/daemon/src/adapter" && existsSync(path.join(root, "apps/daemon/src/providers", `${entry.name}.rs`))) continue;
          walk(file);
        } else if (entry.isFile() && /\.(?:rs|tsx?|m?js|css)$/.test(file) && !/(?:\.test\.|_tests?\.rs$|\/tests\.rs$|\/(?:fixtures?|test_support)\.rs$|TestSupport\.|TestState\.|\/testSupport\.|\/test_provider\.rs$)/.test(file)) {
          failures.push(...violations(file, readFileSync(path.join(root, file), "utf8")));
        }
      }
    };
    walk(base);
  }
  return failures;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const failures = checkProviderNeutral(process.cwd());
  for (const hit of failures) console.error(`${hit.file}:${hit.line}: provider knowledge: ${hit.text}`);
  if (failures.length) process.exitCode = 1;
  else console.log("Provider-neutral core check passed.");
}
