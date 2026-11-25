// Search production core code for provider knowledge; the only exceptions are the registered
// providers' adapter registration lines.
import { readdirSync, readFileSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// The spellings each provider is known by besides its id: product names, quoted name forms,
// dot-directories, instruction files, environment variables (a trailing `_` is a prefix), CLI flags,
// URL schemes, hook event names and other file names. One entry per provider id; an entry applies
// whether or not its descriptor is present, and a registered provider with no entry is still
// guarded by its id.
export const VOCABULARY = {
  claude: {
    names: ["Claude"],
    quoted: ["claude-code"],
    directories: [".claude"],
    files: ["CLAUDE.md"],
    environment: ["CLAUDECODE", "CLAUDE_"],
    flags: ["--permission-mode", "--dangerously-skip-permissions"],
    schemes: ["claude"],
    events: ["PreToolUse", "PostToolUse", "UserPromptSubmit"],
  },
  codex: {
    names: ["Codex"],
    directories: [".codex"],
    files: ["AGENTS.md", "config.toml", "auth.json"],
    environment: ["CODEX_"],
    flags: ["--approval-policy", "--ask-for-approval", "--dangerously-bypass-approvals-and-sandbox"],
    schemes: ["codex"],
  },
  grok: {
    names: ["Grok"],
    quoted: ["grok-build"],
    directories: [".grok"],
    environment: ["GROK_"],
    schemes: ["grok"],
    events: ["StopCancelled", "idle_prompt"],
  },
  antigravity: {
    names: ["Antigravity"],
    quoted: ["agy"],
    directories: [".gemini"],
    environment: ["AGY_", "ANTIGRAVITY_"],
  },
  copilot: {
    names: ["Copilot"],
    directories: [".copilot"],
    environment: ["COPILOT_"],
  },
  opencode: {
    names: ["OpenCode"],
    directories: [".opencode"],
    files: ["opencode.json", "opencode.jsonc"],
    environment: ["OPENCODE_"],
  },
  qwen: {
    names: ["Qwen"],
    directories: [".qwen"],
    files: ["QWEN.md"],
    environment: ["QWEN_"],
  },
  // Only spellings that cannot collide with ordinary code: its quoted id and id constant (from the id), its
  // dot-directory and its variable prefix; a bare `Pi` or `pi` is ordinary text.
  pi: {
    directories: [".pi"],
    environment: ["PI_"],
  },
};

const DESCRIPTORS = "apps/daemon/src/providers";
const NOT_DESCRIPTORS = new Set(["mod.rs", "layout.rs", "test_provider.rs"]);
const ADAPTER_REGISTRY = "apps/daemon/src/adapter/mod.rs";

// The ids of the registered providers: one descriptor file per provider, named for its id.
export function providerIds(root) {
  const directory = path.join(root, DESCRIPTORS);
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs") && !NOT_DESCRIPTORS.has(entry.name))
    .map((entry) => entry.name.slice(0, -3))
    .sort();
}

const escape = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const word = (text) => `${/^\w/.test(text) ? "\\b" : ""}${escape(text)}${/\w$/.test(text) ? "\\b" : ""}`;

// The search for provider knowledge: the ids of `ids` and of every vocabulary entry, as constants
// and quoted, and every vocabulary spelling.
export function knowledgePattern(ids = []) {
  const all = [...new Set([...Object.keys(VOCABULARY), ...ids])];
  const alternatives = [];
  for (const id of all) {
    const entry = VOCABULARY[id] ?? {};
    alternatives.push(`AgentId\\s*::\\s*${escape(id.toUpperCase())}\\b`);
    for (const quoted of [id, ...(entry.quoted ?? [])]) alternatives.push(`["'\`]${escape(quoted)}\\\\?["'\`]`);
    for (const name of entry.names ?? []) alternatives.push(word(name));
    for (const directory of entry.directories ?? []) alternatives.push(`${escape(directory)}\\b`);
    for (const file of entry.files ?? []) alternatives.push(word(file));
    for (const variable of entry.environment ?? []) {
      alternatives.push(variable.endsWith("_") ? `\\b${escape(variable)}[A-Z0-9_]+\\b` : word(variable));
    }
    for (const flag of entry.flags ?? []) alternatives.push(escape(flag));
    for (const scheme of entry.schemes ?? []) alternatives.push(`${escape(scheme)}:\\/\\/`);
    for (const event of entry.events ?? []) alternatives.push(word(event));
  }
  return new RegExp(alternatives.join("|"), "g");
}

// The one permitted registration point: each registered provider's line in the adapter registry,
// pairing its id with its implementation, with or without a `cfg` attribute before it.
export function registrationExceptions(ids) {
  return ids.map((id) => ({
    file: ADAPTER_REGISTRY,
    pattern: new RegExp(`^(?:#\\[cfg\\([^\\]]*\\)\\]\\s*)?\\(AgentId::${escape(id.toUpperCase())},\\s*&${escape(id)}::[A-Z]\\w*\\),$`),
    reason: "The provider adapter registry is the one permitted registration point.",
  }));
}

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

export function violations(file, source, { ids = Object.keys(VOCABULARY), exceptions = registrationExceptions(ids) } = {}) {
  const production = productionSource(source, file.endsWith(".rs"), false, file.endsWith(".css"));
  const lines = source.split("\n");
  const hitLines = new Set([...production.matchAll(knowledgePattern(ids))].map((match) => production.slice(0, match.index).split("\n").length - 1));
  return [...hitLines].flatMap((index) => {
    const line = lines[index].trim();
    if (exceptions.some((exception) => exception.file === file && exception.pattern.test(line) && exception.reason)) return [];
    return [{ file, line: index + 1, text: line }];
  });
}

export function checkProviderNeutral(root) {
  const failures = [];
  const ids = providerIds(root);
  const options = { ids, exceptions: registrationExceptions(ids) };
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
          failures.push(...violations(file, readFileSync(path.join(root, file), "utf8"), options));
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
