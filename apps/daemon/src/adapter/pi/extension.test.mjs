// Tests of Octoboard's Pi extension (`extension.js`) against a fake of Pi's extension API, a stub
// MCP server and a hook script that keeps what it is given. Run with
// `node --test apps/daemon/src/adapter/pi/extension.test.mjs`; POSIX only, for the hook script.

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const EXTENSION = join(dirname(fileURLToPath(import.meta.url)), "extension.js");

// Answers the handshake, offers one tool, `report`, and answers a call with what it was called with.
const STUB_SERVER = `
import { createInterface } from "node:readline";
createInterface({ input: process.stdin }).on("line", (line) => {
  const message = JSON.parse(line);
  if (message.id === undefined) return;
  const result =
    message.method === "initialize"
      ? { protocolVersion: message.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "stub", version: "0" } }
      : message.method === "tools/list"
        ? { tools: [{ name: "report", description: "Report.", inputSchema: { type: "object", properties: {} } }] }
        : { content: [{ type: "text", text: "stub:" + message.params.name + ":" + JSON.stringify(message.params.arguments) }] };
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: message.id, result }) + "\\n");
});
`;

// Never answers `initialize`. It keeps the method of every message it is given, and once the file
// `go` is beside it, asks for a ping and writes `pong` as the answer comes: whatever the client
// had sent by then is in `received` before it.
const SILENT_SERVER = `
import { appendFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
const here = dirname(fileURLToPath(import.meta.url));
createInterface({ input: process.stdin }).on("line", (line) => {
  const message = JSON.parse(line);
  appendFileSync(join(here, "received"), (message.method ?? "pong") + "\\n");
});
const poll = setInterval(() => {
  if (!existsSync(join(here, "go"))) return;
  clearInterval(poll);
  process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: "after", method: "ping" }) + "\\n");
}, 10);
`;

// How long a wait on the hook script or the stub server may take before the test fails.
const PATIENCE_MS = 10_000;

// The limit of `extension.js` on the message it forwards, in UTF-16 units.
const MESSAGE_LIMIT = 200_000;

// Waits, up to the patience, until `read` returns something.
async function eventually(read, what) {
  const until = Date.now() + PATIENCE_MS;
  for (;;) {
    const value = read();
    if (value) return value;
    assert.ok(Date.now() < until, what());
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

// Loads the extension as the daemon writes it, into a scratch directory of its own, with the
// user's and the project's `mcp.json` as given, and returns a fake of Pi that it registered with.
// `server` is the stub's source, `command` what runs it and `timeout` the seconds a request of the
// extension's own client may take.
// `hookDelay` is how long, in seconds, the hook script takes before it keeps a payload and exits,
// and `hookWaitMs` how long a run's prompt waits for it. `changed` replaces members of the fake
// before the extension is loaded.
async function load({
  name = "octoboard",
  users,
  project,
  server = STUB_SERVER,
  command = process.execPath,
  timeout = 30,
  guard,
  hookDelay = 0,
  hookWaitMs = 2000,
  channel,
  reportTurnStatus = false,
  changed = {},
} = {}) {
  const dir = mkdtempSync(join(tmpdir(), "octoboard-pi-extension-"));
  for (const sub of ["scratch", "agent", "cwd/.pi", "hooks"]) mkdirSync(join(dir, sub), { recursive: true });
  writeFileSync(join(dir, "stub.mjs"), server);
  // Each payload goes to a file of its own, so two hooks running at once cannot mix theirs. The
  // delay is a process naming the scratch directory, so `endHookScripts` can find it.
  const delay = `"${process.execPath}" -e "setTimeout(() => {}, ${hookDelay * 1000})" "${dir}"`;
  writeFileSync(
    join(dir, "hook.sh"),
    `#!/bin/sh\ncat > "${dir}/hooks/$$.tmp" && ${delay} && mv "${dir}/hooks/$$.tmp" "${dir}/hooks/$$.json"\n`,
  );
  chmodSync(join(dir, "hook.sh"), 0o755);
  // The guard script, where the test gives one: its source, given what the extension asks on stdin.
  if (guard !== undefined) {
    writeFileSync(join(dir, "guard.sh"), guard);
    chmodSync(join(dir, "guard.sh"), 0o755);
  }
  if (channel) {
    writeFileSync(join(dir, "channel.sh"), channel);
    chmodSync(join(dir, "channel.sh"), 0o755);
  }
  copyFileSync(EXTENSION, join(dir, "scratch", "octoboard-pi.js"));
  // Pi loads the file as a module whatever is beside it; Node goes by the nearest `package.json`.
  writeFileSync(join(dir, "scratch", "package.json"), JSON.stringify({ type: "module" }));
  writeFileSync(
    join(dir, "scratch", "octoboard-pi.json"),
    // The daemon's own facts as it writes them (`write_extension`, pinned by its tests).
    JSON.stringify({
      hook: join(dir, "hook.sh"),
      hook_wait_ms: hookWaitMs,
      report_turn_status: reportTurnStatus,
      channel: channel ? { command: join(dir, "channel.sh"), args: [] } : undefined,
      guard: guard === undefined ? null : join(dir, "guard.sh"),
      guard_timeout_ms: 10_000,
      file_writing_tools: ["write", "edit"],
      mcp: { name, command, args: [join(dir, "stub.mjs")], timeout },
      pasted: {
        directory: "pasted",
        prefix: "paste-",
        types: { png: "image/png", jpg: "image/jpeg", gif: "image/gif", webp: "image/webp" },
      },
    }),
  );
  const entry = { mcpServers: { [users ?? project]: { command: "true" } } };
  if (users) writeFileSync(join(dir, "agent", "mcp.json"), JSON.stringify(entry));
  if (project) writeFileSync(join(dir, "cwd", ".pi", "mcp.json"), JSON.stringify(entry));
  process.env.PI_CODING_AGENT_DIR = join(dir, "agent");
  process.chdir(join(dir, "cwd"));

  const handlers = new Map();
  const pi = {
    dir,
    servers: [],
    tools: new Map(),
    // The file and the id of the session Pi has open; the file is not written until a test writes it.
    session: { file: join(dir, "session.jsonl"), id: "unsaved" },
    on: (event, handler) => handlers.set(event, [...(handlers.get(event) ?? []), handler]),
    registerMcpServer: (server, config) => pi.servers.push({ server, config }),
    registerTool: (definition) => pi.tools.set(definition.name, definition),
    async emit(event, payload = {}) {
      await pi.answers(event, payload);
    },
    // What each handler of `event` answered, in order.
    async answers(event, payload = {}) {
      const ctx = {
        cwd: process.cwd(),
        ...pi.context,
        sessionManager: { getSessionFile: () => pi.session.file, getSessionId: () => pi.session.id },
      };
      const answered = [];
      for (const handler of handlers.get(event) ?? []) answered.push(await handler(payload, ctx));
      return answered;
    },
    handled: (event) => (handlers.get(event) ?? []).length,
    // The payloads the hook script was given, once there are `count` of them.
    async forwarded(count) {
      const files = () => readdirSync(join(dir, "hooks")).filter((file) => file.endsWith(".json"));
      await eventually(
        () => files().length >= count,
        () => `the hook script ran ${files().length} times, not ${count}`,
      );
      return files().map((file) => JSON.parse(readFileSync(join(dir, "hooks", file), "utf8")));
    },
  };
  // What a test changes of the fake, as a Pi release with another extension interface would.
  Object.assign(pi, changed);
  (await import(pathToFileURL(join(dir, "scratch", "octoboard-pi.js")).href)).default(pi);
  return pi;
}

test("Pi reports its live projection at settle, rewind and compaction without another prompt", async () => {
  const pi = await load();
  let usage = { tokens: 3485, contextWindow: 60000 };
  pi.context = { getContextUsage: () => usage };
  await pi.emit("agent_settled");
  usage = { tokens: 140, contextWindow: 60000 };
  await pi.emit("session_tree");
  usage = { tokens: null, contextWindow: 60000 };
  await pi.emit("session_compact");
  const reports = (await pi.forwarded(4)).filter((event) => event.hook_event_name === "context_usage");
  assert.deepEqual(reports.map(({ tokens, window }) => [tokens, window]), [[3485, 60000], [140, 60000], [null, 60000]]);
  await pi.emit("session_shutdown");
});

// Pi exits as it starts on an extension that throws while loading (1.5.2, the capture
// `pi_extension_throws_1.5.2.bin`), so a Pi whose extension interface has changed must still load
// this one: with Octoboard's additions off, never with Pi ended.
test("loading never throws into Pi, whatever its extension interface has lost", async () => {
  for (const changed of [
    { on: undefined },
    { on: () => { throw new Error("changed"); } },
    { registerMcpServer: undefined, registerTool: undefined },
  ]) {
    await load({ changed });
  }
});

test("a handler that throws on what Pi gives it answers nothing instead of throwing into Pi", async () => {
  const pi = await load();
  const changed = { get aborted() { throw new Error("changed"); } };
  const answers = await pi.answers("agent_settled", changed);
  assert.ok(answers.length > 0);
  assert.deepEqual(answers, answers.map(() => undefined));
  await pi.emit("session_shutdown");
});

test("the server is registered with Pi where no mcp.json names it", async () => {
  const pi = await load({ users: "another" });
  assert.deepEqual(
    pi.servers.map(({ server, config }) => [server, config.exposure, config.timeout]),
    [["octoboard", "direct", 30]],
  );
  await pi.emit("session_start", { reason: "startup" });
  await pi.emit("before_agent_start");
  assert.equal(pi.tools.size, 0);
  await pi.emit("session_shutdown");
});

for (const [where, options] of [
  ["the user's mcp.json", { users: "octoboard" }],
  ["the project's mcp.json, under a name that differs in a dash", { name: "octo_board", project: "octo-board" }],
]) {
  test(`the tools are registered by the extension itself where ${where} names the server`, async () => {
    const pi = await load(options);
    assert.deepEqual(pi.servers, [], "nothing is registered for the user's server to replace");
    await pi.emit("session_start", { reason: "startup" });
    // The prompt waits for the tools, so its first request declares them.
    await pi.emit("before_agent_start");
    const name = `mcp__${options.name ?? "octoboard"}__report`;
    assert.deepEqual([...pi.tools.keys()], [name]);
    const result = await pi.tools.get(name).execute("call-1", { status: "done" }, undefined);
    assert.deepEqual(result.content, [{ type: "text", text: 'stub:report:{"status":"done"}' }]);
    await pi.emit("session_shutdown");
  });
}

test("a server that cannot be started leaves the session without the tools and Pi unharmed", async () => {
  // Node refuses an empty command as it is asked to start it.
  const pi = await load({ users: "octoboard", command: "" });
  await pi.emit("session_start", { reason: "startup" });
  await pi.emit("before_agent_start");
  assert.equal(pi.tools.size, 0);
  await pi.emit("session_shutdown");
});

test("an initialize that timed out is not cancelled", async () => {
  const pi = await load({ users: "octoboard", server: SILENT_SERVER, timeout: 0.05 });
  await pi.emit("session_start", { reason: "startup" });
  // Returns once the registration has failed, at the timeout of `initialize`.
  await pi.emit("before_agent_start");
  assert.equal(pi.tools.size, 0);
  writeFileSync(join(pi.dir, "go"), "");
  const received = () => (existsSync(join(pi.dir, "received")) ? readFileSync(join(pi.dir, "received"), "utf8") : "");
  await eventually(
    () => received().includes("pong"),
    () => `the stub server was given no answer to its ping: ${received()}`,
  );
  assert.deepEqual(received().trim().split("\n"), ["initialize", "pong"]);
  await pi.emit("session_shutdown");
});

test("a message cut at the limit does not end inside a character", async () => {
  const pi = await load();
  // The limit falls between the two halves of the emoji.
  const text = `${"a".repeat(MESSAGE_LIMIT - 1)}\u{1F600} and more`;
  await pi.emit("agent_end", { messages: [{ role: "assistant", content: [{ type: "text", text }], stopReason: "stop" }] });
  await pi.emit("agent_settled", { aborted: false });
  const [payload] = await pi.forwarded(1);
  assert.equal(payload.last_assistant_message, "a".repeat(MESSAGE_LIMIT - 1));
});

test("a session with no file at its start is named once a run ends with its file written", async () => {
  const pi = await load();
  await pi.emit("session_start", { reason: "fork" });
  writeFileSync(pi.session.file, "");
  pi.session.id = "forked";
  await pi.emit("agent_end", { messages: [] });
  const named = (await pi.forwarded(2)).map((payload) => [payload.hook_event_name, payload.reason, payload.session_id]);
  assert.deepEqual(named.sort(), [
    ["session_start", "fork", undefined],
    ["session_start", "saved", "forked"],
  ]);
});

test("what a run said is forwarded as Pi settles, and nothing for a cancelled run", async () => {
  const pi = await load();
  const assistant = (content, extra = {}) => ({ role: "assistant", content, stopReason: "stop", ...extra });
  const settle = async (messages, aborted = false) => {
    await pi.emit("agent_end", { messages });
    await pi.emit("agent_settled", { aborted });
  };
  // A run that called a tool and then answered: the last assistant message's text.
  await settle([
    { role: "user", content: [{ type: "text", text: "do it" }] },
    assistant([{ type: "text", text: "calling" }, { type: "toolCall", id: "1", name: "bash", arguments: {} }]),
    { role: "toolResult", content: [{ type: "text", text: "ok" }] },
    assistant([{ type: "text", text: "all done" }]),
  ]);
  // A cancelled run forwards nothing, and its text is not kept for a settling that follows no
  // run, as a manual `/compact` settles.
  await settle([assistant([{ type: "text", text: "cut short" }], { stopReason: "aborted" })], true);
  await pi.emit("agent_settled", { aborted: false });
  await settle([assistant([])]);
  await settle([assistant([], { stopReason: "error", errorMessage: "400: refused" })]);
  const said = (await pi.forwarded(4)).map((payload) => {
    assert.equal(payload.hook_event_name, "agent_settled");
    return payload.last_assistant_message;
  });
  assert.deepEqual(said.map(String).sort(), ["400: refused", "all done", "null", "null"]);
});

test("the fallback reports working, user waits, completion and cancellation", async () => {
  const pi = await load({ reportTurnStatus: true });
  await pi.emit("agent_start");
  await pi.emit("ui_prompt_start");
  await pi.emit("ui_prompt_end");
  await pi.emit("agent_end", {
    messages: [{ role: "assistant", content: "finished", stopReason: "stop" }],
  });
  await pi.emit("agent_settled", { aborted: false });
  await pi.emit("agent_start");
  await pi.emit("agent_end", {
    messages: [{ role: "assistant", content: "discarded", stopReason: "aborted" }],
  });
  await pi.emit("agent_settled", { aborted: true });

  const reports = await pi.forwarded(6);
  assert.deepEqual(reports.map(({ turn_status }) => turn_status), [
    "working", "blocked", "working", "done", "working", "idle",
  ]);
  assert.equal(reports[3].last_assistant_message, "finished");
  assert.equal(reports[5].last_assistant_message, undefined);
});

test("each user message that enters the conversation is forwarded as a prompt, and nothing else is", async () => {
  const pi = await load();
  await pi.emit("message_start", { message: { role: "user", content: [{ type: "text", text: "do " }, { type: "image" }, { type: "text", text: "it" }] } });
  await pi.emit("message_start", { message: { role: "assistant", content: [{ type: "text", text: "said" }] } });
  await pi.emit("message_start", { message: { role: "user", content: "steer" } });
  const prompts = (await pi.forwarded(2)).map((payload) => {
    assert.equal(payload.hook_event_name, "message_start");
    return payload.prompt;
  });
  assert.deepEqual(prompts.sort(), ["do it", "steer"]);
});

// Ends the hook scripts of `pi` still running, and their delays, so none outlives the test: every
// process whose command line names its scratch directory.
function endHookScripts(pi) {
  spawnSync("pkill", ["-f", pi.dir]);
}

// The payloads the hook script has kept so far.
function kept(pi) {
  return readdirSync(join(pi.dir, "hooks"))
    .filter((file) => file.endsWith(".json"))
    .map((file) => JSON.parse(readFileSync(join(pi.dir, "hooks", file), "utf8")));
}

test("a run's prompt is forwarded before the run starts, once the hook script has exited, and not again", async () => {
  // Pi reports `working` only once `before_agent_start` has returned: the hook script has exited
  // by then, so the daemon has the prompt before the report, and a turn that ends at once (a
  // refused request, an instant Esc) cannot be closed before its prompt arrives.
  // The wait is far longer than any hook takes, so only the script's exit ends it, however slow
  // the machine.
  const pi = await load({ hookDelay: 0.3, hookWaitMs: 60_000 });
  await pi.emit("before_agent_start", { type: "before_agent_start", prompt: "do it", images: [] });
  assert.deepEqual(kept(pi), [{ hook_event_name: "message_start", prompt: "do it" }]);
  // The run's own message, then a steering message Pi takes into the run.
  await pi.emit("message_start", { message: { role: "user", content: "do it" } });
  await pi.emit("message_start", { message: { role: "user", content: "steer" } });
  const prompts = (await pi.forwarded(2)).map((payload) => payload.prompt);
  assert.deepEqual(prompts.sort(), ["do it", "steer"]);
});

test("a run's prompt waits for the hook script no longer than its bound", async () => {
  // The hook takes far longer than the test may, so the emit resolving at all is the bound at
  // work, and the script has kept nothing by then.
  const pi = await load({ hookDelay: 30, hookWaitMs: 100 });
  await pi.emit("before_agent_start", { type: "before_agent_start", prompt: "do it" });
  assert.deepEqual(kept(pi), []);
  endHookScripts(pi);
});

test("a run cancelled before its own message leaves no prompt to skip for a later one", async () => {
  const pi = await load();
  // Cancelled while it was starting: Pi ends and settles it with no `message_start` of its prompt.
  await pi.emit("before_agent_start", { type: "before_agent_start", prompt: "cancelled" });
  await pi.emit("agent_end", { messages: [] });
  await pi.emit("agent_settled", { aborted: true });
  // A user message that enters the conversation afterwards with no run started for it.
  await pi.emit("message_start", { message: { role: "user", content: "steer" } });
  const prompts = (await pi.forwarded(2)).map((payload) => payload.prompt);
  assert.deepEqual(prompts.sort(), ["cancelled", "steer"]);
});

test("a run of an extension's custom message skips none of its user messages, after a run that kept its own", async () => {
  const pi = await load();
  // A run whose prompt was forwarded and whose own message never came, with no end seen.
  await pi.emit("before_agent_start", { type: "before_agent_start", prompt: "first" });
  await pi.emit("before_agent_start", { type: "before_agent_start" });
  await pi.emit("message_start", { message: { role: "user", content: "steer" } });
  const prompts = (await pi.forwarded(2)).map((payload) => payload.prompt);
  assert.deepEqual(prompts.sort(), ["first", "steer"]);
});

test("a run of an extension's custom message forwards no prompt", async () => {
  const pi = await load();
  await pi.emit("before_agent_start", { type: "before_agent_start" });
  await pi.emit("message_start", { message: { role: "user", content: "typed" } });
  const prompts = (await pi.forwarded(1)).map((payload) => payload.prompt);
  assert.deepEqual(prompts, ["typed"]);
});

// Keeps what it is asked in `asked.jsonl` beside it and, after a pause, refuses a path under a
// directory named `other`, as Octoboard's guard answers.
const REFUSING_GUARD = `#!/bin/sh
in=$(cat)
printf '%s\\n' "$in" >> "$(dirname "$0")/asked.jsonl"
sleep 0.2
case "$in" in *'/other/'*) printf '{"block":true,"reason":"Blocked: not yours"}\\n';; esac
exit 0
`;

test("a write or an edit the guard refuses is refused with its reason, and the guard is given the resolved path", async () => {
  const pi = await load({ guard: REFUSING_GUARD });
  const cwd = process.cwd();
  assert.deepEqual(await pi.answers("tool_call", { toolName: "write", input: { path: "../other/x.txt", content: "" } }), [
    { block: true, reason: "Blocked: not yours" },
  ]);
  assert.deepEqual(await pi.answers("tool_call", { toolName: "edit", input: { path: "@mine.txt", edits: [] } }), [undefined]);
  // Another tool is not asked about.
  assert.deepEqual(await pi.answers("tool_call", { toolName: "bash", input: { command: "touch ../other/x" } }), [undefined]);
  const asked = readFileSync(join(pi.dir, "asked.jsonl"), "utf8").trim().split("\n").map((line) => JSON.parse(line));
  assert.deepEqual(asked, [
    { tool_name: "write", tool_input: { path: join(dirname(cwd), "other", "x.txt") }, cwd },
    { tool_name: "edit", tool_input: { path: join(cwd, "mine.txt") }, cwd },
  ]);
});

test("a guard that cannot decide lets the call go ahead", async () => {
  for (const guard of ["#!/bin/sh\ncat >/dev/null; echo 'not json'\n", "#!/bin/sh\ncat >/dev/null; echo '{\"block\":true}'\n", "#!/bin/sh\nexit 3\n"]) {
    const pi = await load({ guard });
    assert.deepEqual(await pi.answers("tool_call", { toolName: "write", input: { path: "/other/x" } }), [undefined], guard);
  }
});

test("a session with no guard script asks nothing about its calls", async () => {
  const pi = await load();
  assert.equal(pi.handled("tool_call"), 0);
});

test("an image Octoboard pasted into the session is attached, and its path taken out of the text", async () => {
  const pi = await load();
  const pasted = join(pi.dir, "scratch", "pasted");
  mkdirSync(pasted);
  writeFileSync(join(pasted, "paste-1-0.png"), "PNG");
  writeFileSync(join(pasted, "paste-2-0.jpg"), "JPG");
  const one = join(pasted, "paste-1-0.png");
  const two = join(pasted, "paste-2-0.jpg");
  const [answer] = await pi.answers("input", { text: `${one}${two}what is this`, images: [], source: "interactive" });
  assert.deepEqual(answer, {
    action: "transform",
    text: "what is this",
    images: [
      { type: "image", mimeType: "image/png", data: Buffer.from("PNG").toString("base64") },
      { type: "image", mimeType: "image/jpeg", data: Buffer.from("JPG").toString("base64") },
    ],
  });
});

test("a path that is not an image Octoboard pasted for this session stays text", async () => {
  const pi = await load();
  const elsewhere = join(pi.dir, "pasted");
  mkdirSync(elsewhere);
  writeFileSync(join(elsewhere, "paste-1-0.png"), "PNG");
  mkdirSync(join(pi.dir, "scratch", "pasted"));
  for (const text of [`${join(elsewhere, "paste-1-0.png")} look`, `${join(pi.dir, "scratch", "pasted", "paste-9-0.png")} gone`, "plain text"]) {
    assert.deepEqual(await pi.answers("input", { text, source: "interactive" }), [undefined], text);
  }
});



test("an accepted channel message is returned only after Pi's restored editor loses it", async (t) => {
  const script = `#!/bin/sh
cd "$(dirname "$0")" || exit 1
printf '%s\n' '{"id":"request-1","text":"Channel message"}'
cat > outcomes
`;
  const pi = await load({ channel: script });
  t.after(() => pi.emit("session_shutdown"));
  let editor = "A user's draft";
  let pending = false;
  let terminalInput;
  pi.context = {
    hasPendingMessages: () => pending,
    ui: {
      getEditorText: () => editor,
      setEditorText: (text) => { editor = text; },
      onTerminalInput: (handler) => {
        terminalInput = handler;
        return () => { terminalInput = undefined; };
      },
    },
  };
  pi.sendUserMessage = async (text, options) => {
    assert.deepEqual(options, { deliverAs: "followUp", expandPromptTemplates: false });
    await pi.emit("input", { text, source: "extension", streamingBehavior: "followUp" });
    pending = true;
  };
  await pi.emit("session_start", { reason: "startup" });
  const file = join(pi.dir, "outcomes");
  const outcomes = () => existsSync(file)
    ? readFileSync(file, "utf8").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line))
    : [];
  await eventually(() => outcomes().length === 1, () => "the helper has no acceptance");
  assert.deepEqual(outcomes(), [{ id: "request-1", outcome: "taken" }]);

  terminalInput("\x1b");
  editor = "Channel message\n\nA user's draft";
  pending = false;
  await pi.emit("agent_settled", { aborted: true });
  await eventually(() => outcomes().length === 2, () => "the helper has no return");
  assert.deepEqual(outcomes()[1], { id: "request-1", outcome: "returned" });
  assert.equal(editor, "A user's draft");
});
