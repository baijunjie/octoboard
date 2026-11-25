import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { checkProviderNeutral, knowledgePattern, providerIds, registrationExceptions, VOCABULARY, violations } from "./provider-neutral.mjs";

const DAEMON = "apps/daemon/src/session/example.rs";
const UI = "packages/ui/src/example.ts";

test("rejects reintroduced agent-id branches and provider facts in generic code", () => {
  for (const source of [
    "if agent == AgentId::CLAUDE { act(); }",
    "match agent { AgentId::CODEX => act(), _ => () }",
    'if (agent === "grok") act();',
    'const file = "AGENTS.md";',
    'let directory = ".codex";',
    'let variable = "GROK_WORKSPACE_ROOT";',
    'let args = ["--dangerously-skip-permissions"];',
    'const remedy = "Reinstall Codex.";',
    'let event = "PreToolUse";',
    'const name = "Claude";',
    'const name = "Grok";',
    "const id = `codex`;",
    'const variable = "CODEX_V2_SESSION";',
    'let text = "the agent (such as `\\"claude\\"`)";',
    "let text = 'an id like \\'grok\\'';",
  ]) assert.equal(violations(DAEMON, source).length, 1, source);
  assert.equal(violations(UI, 'if (agent === "codex") act();').length, 1);
  assert.equal(violations(DAEMON, "provider(agent).screen().interrupt()").length, 0);
});

test("every spelling a provider is known by is guarded, one per kind", () => {
  for (const source of [
    'let id = "claude-code";',
    'let id = "grok-build";',
    'let id = "claude";',
    "let directory = '.claude';",
    'let directory = ".grok";',
    'let file = "CLAUDE.md";',
    'let file = "config.toml";',
    'let file = "auth.json";',
    'let variable = "CLAUDECODE";',
    'let variable = "CLAUDE_CONFIG_DIR";',
    'let variable = "CODEX_HOME";',
    'let args = ["--permission-mode", "plan"];',
    'let args = ["--approval-policy"];',
    'let args = ["--ask-for-approval"];',
    'let args = ["--dangerously-bypass-approvals-and-sandbox"];',
    'let url = "claude://open";',
    'let url = "codex://threads";',
    'let url = "grok://open";',
    'let event = "PostToolUse";',
    'let event = "UserPromptSubmit";',
    'let event = "StopCancelled";',
    'let notification = "idle_prompt";',
  ]) assert.equal(violations(DAEMON, source).length, 1, source);
  // Only whole words and names: the neighbours of a spelling are not it.
  for (const source of [
    'let name = "Claudette";',
    'let name = "Grokking";',
    'let file = "AGENTS.mdx";',
    'let variable = "CLAUDECODE2";',
    'let variable = "CLAUDE_lower";',
    'let event = "PreToolUseHandler";',
    "let id = claude;",
  ]) assert.equal(violations(DAEMON, source).length, 0, source);
});

test("the vocabulary is per provider, and a registered provider without an entry is guarded by its id", () => {
  assert.deepEqual(Object.keys(VOCABULARY).sort(), ["antigravity", "claude", "codex", "copilot", "grok", "opencode", "pi", "qwen"]);
  const pattern = (ids) => knowledgePattern(ids);
  const hits = (ids, source) => [...source.matchAll(pattern(ids))].length;
  assert.equal(hits(["fifth"], "AgentId::FIFTH"), 1);
  assert.equal(hits(["fifth"], 'x = "fifth"'), 1);
  assert.equal(hits([], 'x = "fifth"'), 0);
  // The entries apply whether or not their descriptors are among the ids.
  assert.equal(hits([], "AgentId::CODEX"), 1);
  assert.equal(hits(["fifth"], "Codex"), 1);
  // Nothing of a provider without an entry is guarded beyond its id.
  assert.equal(hits(["fifth"], "Fifth .fifth FIFTH_HOME"), 0);
  assert.equal(violations(DAEMON, 'if agent == "fifth" {}', { ids: ["fifth"] }).length, 1);
});

test("every preview provider is guarded by its id, and by its own spellings where they cannot collide with ordinary code", () => {
  const spellings = {
    antigravity: ['"antigravity"', "AgentId::ANTIGRAVITY", 'let name = "Antigravity";', 'let binary = "agy";', 'let directory = ".gemini";', 'let variable = "AGY_CLI_OSC7501";'],
    copilot: ['"copilot"', "AgentId::COPILOT", 'let name = "Copilot";', 'let directory = ".copilot";', 'let variable = "COPILOT_HOME";'],
    opencode: [
      '"opencode"', "AgentId::OPENCODE", 'let name = "OpenCode";', 'let directory = ".opencode";',
      'let file = "opencode.json";', 'let file = "opencode.jsonc";', 'let variable = "OPENCODE_CONFIG";',
    ],
    qwen: ['"qwen"', "AgentId::QWEN", 'let name = "Qwen";', 'let directory = ".qwen";', 'let file = "QWEN.md";', 'let variable = "QWEN_HOME";'],
    pi: ['"pi"', "'pi'", "`pi`", "AgentId::PI", 'let directory = ".pi/agent";', 'let variable = "PI_CODING_AGENT_DIR";'],
  };
  for (const [id, sources] of Object.entries(spellings)) {
    for (const source of sources) {
      assert.equal(violations(DAEMON, source).length, 1, `${id}: ${source}`);
      assert.equal(violations(UI, source).length, 1, `${id}: ${source}`);
    }
    assert.equal(violations(DAEMON, `if agent == "${id}" { act(); }`).length, 1, id);
    assert.equal(violations(DAEMON, `if agent == AgentId::${id.toUpperCase()} { act(); }`).length, 1, id);
  }
});

test("a bare Pi or pi in ordinary text or code is not reported, and neither are its neighbours", () => {
  for (const source of [
    "let pi = 3.14159;",
    "const Pi = 3;",
    "const area = Math.PI * radius * radius;",
    "// Pi is a number",
    "let text = \"Pi day\";",
    "const API_KEY = read();",
    "let name = \"Pip\";",
    'let directory = ".pixel";',
    'let variable = "PI_lower";',
    "let spirit = pipeline(pi);",
  ]) assert.equal(violations(DAEMON, source).length, 0, source);
});

test("exceptions allow each preview provider's registration line, with its gate, at the adapter registry only", () => {
  const registry = "apps/daemon/src/adapter/mod.rs";
  const ids = ["claude", "codex", "grok", "antigravity", "copilot", "opencode", "qwen", "pi"];
  const providers = { antigravity: "AntigravityProvider", copilot: "CopilotProvider", opencode: "OpenCodeProvider", qwen: "QwenProvider", pi: "PiProvider" };
  for (const [id, provider] of Object.entries(providers)) {
    const line = `(AgentId::${id.toUpperCase()}, &${id}::${provider}),`;
    for (const gated of [line, `#[cfg(any(test, debug_assertions))] ${line}`]) {
      assert.equal(violations(registry, gated, { ids }).length, 0, gated);
      assert.equal(violations(DAEMON, gated, { ids }).length, 1, gated);
    }
    assert.equal(violations(registry, `(AgentId::${id.toUpperCase()}, &claude::ClaudeProvider),`, { ids }).length, 1, id);
  }
});

test("comments and test-only Rust items do not hide production after them", () => {
  const source = `// AgentId::CLAUDE
/* "codex" */
#[cfg(all(test, unix))]
mod tests { fn fixture() { let x = "GROK_HOME"; } }
#[cfg(test)]
fn fixture() { let x = r#"{ "agent": "codex" }"#; }
if agent == AgentId::CODEX { act(); }`;
  assert.deepEqual(violations(DAEMON, source).map((hit) => hit.line), [7]);
  assert.equal(violations(UI, 'const text = "// Codex";').length, 1);
});

test("exceptions allow only a registered provider's line at the adapter registry", () => {
  const registry = "apps/daemon/src/adapter/mod.rs";
  const ids = ["claude", "codex", "grok"];
  for (const line of [
    "(AgentId::CLAUDE, &claude::ClaudeProvider),",
    "(AgentId::CODEX, &codex::CodexProvider),",
    "#[cfg(any(test, debug_assertions))] (AgentId::GROK, &grok::GrokProvider),",
  ]) {
    assert.equal(violations(registry, line, { ids }).length, 0, line);
    assert.equal(violations(DAEMON, line, { ids }).length, 1, line);
    assert.equal(violations(registry, `${line} if agent == AgentId::CODEX {}`, { ids }).length, 1, line);
  }
  assert.equal(violations(registry, "(AgentId::CODEX, &claude::ClaudeProvider),", { ids }).length, 1);
  assert.equal(violations(registry, "(AgentId::CODEX, &codex::CodexProvider),", { ids, exceptions: registrationExceptions(["claude"]) }).length, 1);
  for (const exception of registrationExceptions(ids)) assert.ok(exception.reason);
});

test("a provider added by a descriptor file alone is guarded and its registration excepted", (t) => {
  const root = mkdtempSync(path.join(os.tmpdir(), "provider-neutral-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const put = (file, source) => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), source);
  };
  for (const file of ["mod.rs", "layout.rs", "test_provider.rs", "fifth.rs"]) put(`apps/daemon/src/providers/${file}`, "");
  assert.deepEqual(providerIds(root), ["fifth"]);
  put(UI, "callProvider();");
  put("apps/daemon/src/adapter/mod.rs", "const PROVIDERS = [\n    #[cfg(any(test, debug_assertions))]\n    (AgentId::FIFTH, &fifth::FifthProvider),\n];");
  put("apps/daemon/src/adapter/fifth/mod.rs", 'if agent == AgentId::FIFTH { let id = "fifth"; }');
  assert.deepEqual(checkProviderNeutral(root), []);
  put(DAEMON, "if agent == AgentId::FIFTH { act(); }");
  put(UI, 'if (agent === "fifth") act();');
  assert.deepEqual(checkProviderNeutral(root).map((hit) => hit.file).sort(), [DAEMON, UI]);
});

test("the executable scans generic helpers, excludes providers and test fixtures, and fails on a regression", (t) => {
  const root = mkdtempSync(path.join(os.tmpdir(), "provider-neutral-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const put = (file, source) => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), source);
  };
  for (const file of [
    "apps/daemon/src/providers/fourth.rs", "apps/daemon/src/adapter/fourth/mod.rs",
    "apps/daemon/src/session/tests.rs", "apps/daemon/src/session/input_tests.rs",
    "apps/daemon/src/session/fixture.rs", "apps/daemon/src/session/test_support.rs",
    "apps/daemon/src/session/capture.rs", "apps/daemon/src/session/capture/child.rs",
    "packages/ui/src/providers/fourth.ts", "packages/ui/src/gallery/example.ts",
    "packages/ui/src/fixtures/example.ts", "packages/ui/src/website-preview/example.ts",
    "packages/ui/src/example.test.ts", "packages/ui/src/dialogTestSupport.ts",
  ]) put(file, 'if (agent === "codex") act();');
  put("apps/daemon/src/session/mod.rs", "#[cfg(test)]\nmod capture;");
  put(DAEMON, "provider(agent).screen().interrupt()");
  assert.deepEqual(checkProviderNeutral(root), []);
  put("apps/daemon/src/adapter/hooks.rs", 'let event = "PreToolUse";');
  assert.equal(checkProviderNeutral(root).length, 1);
  put("apps/daemon/src/adapter/hooks.rs", "call_provider()");
  for (const file of ["apps/daemon/src/gallery/example.rs", "apps/daemon/src/website-preview/example.rs", "packages/ui/src/feature/gallery/example.ts", "packages/ui/src/feature/website-preview/example.ts", "packages/ui/src/example.css"]) {
    put(file, file.endsWith(".css") ? '[data-agent="codex"] { display: block; }' : 'const agent = "codex";');
    assert.deepEqual(checkProviderNeutral(root).map((hit) => hit.file), [file], file);
    put(file, file.endsWith(".css") ? ".generic { display: block; }" : "call_provider()");
  }
  put(DAEMON, "if agent == AgentId::CODEX { act(); }");
  const result = spawnSync(process.execPath, [path.resolve("scripts/verify/provider-neutral.mjs")], { cwd: root, encoding: "utf8" });
  assert.equal(result.status, 1);
  assert.match(result.stderr, /example\.rs:1: provider knowledge/);
});

test("multiline ids, template strings and production cfg branches remain checked", () => {
  assert.equal(violations(DAEMON, "if agent == AgentId\n ::\n CODEX { act(); }").length, 1);
  assert.equal(violations(UI, "const message = `Reinstall Codex.`;").length, 1);
  for (const cfg of ["any(test, unix)", "not(test)", "all(unix, not(test))"]) {
    assert.equal(violations(DAEMON, `#[cfg(${cfg})]\nfn production() { let agent = AgentId::CODEX; }`).length, 1, cfg);
  }
  for (const cfg of ["test", "all(test, unix)", "all(unix, test)"]) {
    assert.equal(violations(DAEMON, `#[cfg(${cfg})]\nfn fixture() { let agent = AgentId::CODEX; }`).length, 0, cfg);
  }
  assert.equal(violations(DAEMON, 'let fake = "#[cfg(test)]"; let agent = AgentId::CODEX;').length, 1);
  assert.equal(violations(DAEMON, "/* #[cfg(test)] */ let agent = AgentId::CODEX;").length, 1);
});

test("CSS URLs do not turn following selectors into line comments", () => {
  const source = '.safe { background: url(http://localhost/image); } [data-agent="codex"] { color: red; }';
  assert.equal(violations("packages/ui/src/styles/example.css", source).length, 1);
});
