import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { checkProviderNeutral, EXCEPTIONS, violations } from "./provider-neutral.mjs";

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

test("exceptions allow only the exact line at the named registration point", () => {
  for (const exception of EXCEPTIONS) {
    assert.ok(exception.reason);
    assert.equal(violations(exception.file, exception.line).length, 0);
    assert.equal(violations(DAEMON, exception.line).length, 1);
    assert.equal(violations(exception.file, `${exception.line} if agent == AgentId::CODEX {}`).length, 1);
  }
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
