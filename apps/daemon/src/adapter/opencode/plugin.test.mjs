// Tests of Octoboard's OpenCode plugin (`plugin.js`), run as OpenCode runs it: the permission
// rules its `config` hook writes into the configuration OpenCode hands it, against configurations
// of each shape measured on OpenCode 1.19.6 (`docs/agent-cli-reference.md`, "OpenCode"), and what
// its `event` hook forwards and reports for the events of a session. Run with
// `node --test apps/daemon/src/adapter/opencode/plugin.test.mjs`.
//
// The plugin starts the session's hook script for every payload and writes its reports to
// standard output. Both are stood in for here, before the plugin is loaded: no process is
// started and nothing reaches the test's own output.

import assert from "node:assert/strict";
import childProcess from "node:child_process";
import fs from "node:fs";
import { syncBuiltinESMExports } from "node:module";
import { homedir, tmpdir } from "node:os";
import { dirname, join, sep } from "node:path";
import { after, test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const PLUGIN = join(dirname(fileURLToPath(import.meta.url)), "plugin.js");
const TOOLS = "octoboard_**";
// The plugin's own bound on a prompt that is not told of, in milliseconds.
const PROMPT_BOUND_MS = 300;

// The payloads handed to the hook script and the states reported to the terminal, in the order
// the plugin gave them.
const forwarded = [];
const reported = [];

// What the cross-project guard's script is given on its standard input, and what it prints for
// the next call: the script is the one named `GUARD`, any other is the hook script.
const GUARD = "/guard/script";
const guarded = [];
const guard = { answer: "", starts: true, close: true };

// The hook script: takes the payload on its standard input and ends. The guard's script takes the
// payload, prints `guard.answer` and ends; it fails to start where `guard.starts` is false, and
// never ends where `guard.close` is.
childProcess.spawn = (command) => {
  if (command === GUARD) {
    if (!guard.starts) throw new Error("cannot start");
    const handlers = {};
    const output = {};
    return {
      on: (event, handler) => {
        handlers[event] = handler;
      },
      kill: () => {},
      stdout: {
        on: (event, handler) => {
          output[event] = handler;
        },
      },
      stdin: {
        on: () => {},
        end: (body) => {
          guarded.push(JSON.parse(body));
          if (!guard.close) return;
          setImmediate(() => {
            if (guard.answer && output.data) output.data(Buffer.from(guard.answer));
            if (handlers.close) handlers.close(0);
          });
        },
      },
    };
  }
  const handlers = {};
  return {
    on: (event, handler) => {
      handlers[event] = handler;
    },
    kill: () => {},
    stdin: {
      on: () => {},
      end: (body) => {
        forwarded.push(JSON.parse(body));
        setImmediate(() => handlers.close && handlers.close());
      },
    },
  };
};
// The terminal: a write to standard output that is a program status report is kept, whole.
const writeSync = fs.writeSync;
fs.writeSync = (fd, bytes, ...rest) => {
  const text = fd === 1 && Buffer.isBuffer(bytes) ? bytes.toString("latin1") : "";
  const report = /^\u001b\]7501;state=([^\u001b]*)\u001b\\$/.exec(text);
  if (!report) return writeSync(fd, bytes, ...rest);
  reported.push(report[1]);
  return bytes.length;
};
syncBuiltinESMExports();

const directories = [];
after(() => {
  for (const dir of directories) fs.rmSync(dir, { recursive: true, force: true });
});

// Loads the plugin as the daemon writes it, into a scratch directory of its own beside a data file
// holding `data`, and returns its hooks with the directory. `data` of `null` writes no data file.
async function load(data, context = {}) {
  const dir = fs.mkdtempSync(join(tmpdir(), "octoboard-opencode-plugin-"));
  directories.push(dir);
  fs.copyFileSync(PLUGIN, join(dir, "octoboard.js"));
  // OpenCode loads the file as a module whatever is beside it; Node goes by the nearest
  // `package.json`.
  fs.writeFileSync(join(dir, "package.json"), JSON.stringify({ type: "module" }));
  if (data) fs.writeFileSync(join(dir, "octoboard.json"), JSON.stringify(data));
  const module = await import(pathToFileURL(join(dir, "octoboard.js")).href);
  return { dir, hooks: await module.OctoboardPlugin(context) };
}

// The data file of a launch under the approval mode `ask` (`asking`) or under another.
const launch = (asking) => ({
  hook: "/nonexistent/hook",
  user_title_off: null,
  default_action: asking ? "ask" : null,
  tools_rule: TOOLS,
});

// The configuration after the hook of a launch has been given it.
async function written(asking, config) {
  const { hooks } = await load(launch(asking));
  await hooks.config(config);
  return config;
}

// The rules of a `permission` as pairs in the order they are written, which is the order OpenCode
// follows them in.
const inOrder = (permission) => Object.entries(permission);

test("ask goes first where the merged rules write no default, so an allow and a deny of the user's both hold", async () => {
  const config = await written(true, { permission: { bash: "allow", edit: "deny", webfetch: "deny" } });
  assert.deepEqual(inOrder(config.permission), [
    ["*", "ask"],
    [TOOLS, "allow"],
    ["bash", "allow"],
    ["edit", "deny"],
    ["webfetch", "deny"],
  ]);
});

test("ask takes the place of the user's own default where it stands", async () => {
  const config = await written(true, { permission: { bash: "allow", "*": "deny", edit: "deny", read: "allow" } });
  assert.deepEqual(inOrder(config.permission), [
    ["bash", "allow"],
    ["*", "ask"],
    [TOOLS, "allow"],
    ["edit", "deny"],
    ["read", "allow"],
  ]);
});

test("rules by pattern are kept as written, beneath a tool's key and after the default", async () => {
  const config = await written(true, {
    permission: { bash: { "git *": "allow", "rm *": "deny" }, read: { "*.pem": "deny" }, todowrite: "deny" },
  });
  assert.deepEqual(inOrder(config.permission), [
    ["*", "ask"],
    [TOOLS, "allow"],
    ["bash", { "git *": "allow", "rm *": "deny" }],
    ["read", { "*.pem": "deny" }],
    ["todowrite", "deny"],
  ]);
  assert.deepEqual(inOrder(config.permission.bash), [
    ["git *", "allow"],
    ["rm *", "deny"],
  ]);
});

test("a default that holds rules by pattern gets ask among them, and a deny there stays", async () => {
  const none = await written(true, { permission: { "*": { "rm *": "deny" }, edit: "deny" } });
  assert.deepEqual(inOrder(none.permission), [
    ["*", { "*": "ask", "rm *": "deny" }],
    [TOOLS, "allow"],
    ["edit", "deny"],
  ]);
  assert.deepEqual(inOrder(none.permission["*"]), [
    ["*", "ask"],
    ["rm *", "deny"],
  ]);

  const own = await written(true, { permission: { "*": { "rm *": "deny", "*": "allow", "git *": "deny" } } });
  assert.deepEqual(inOrder(own.permission["*"]), [
    ["rm *", "deny"],
    ["*", "ask"],
    ["git *", "deny"],
  ]);
});

test("a permission that is one action is the default, which ask takes the place of", async () => {
  for (const action of ["allow", "deny", "ask"]) {
    const config = await written(true, { permission: action });
    assert.deepEqual(inOrder(config.permission), [
      ["*", "ask"],
      [TOOLS, "allow"],
    ]);
  }
});

test("with no permission at all ask is the only rule beside Octoboard's", async () => {
  const config = await written(true, { theme: "x" });
  assert.deepEqual(inOrder(config.permission), [
    ["*", "ask"],
    [TOOLS, "allow"],
  ]);
  assert.equal(config.theme, "x");
});

test("the rule for Octoboard's tools goes right after the user's default, wherever that stands", async () => {
  const first = await written(false, { permission: { "*": "ask", edit: "deny" } });
  assert.deepEqual(inOrder(first.permission), [
    ["*", "ask"],
    [TOOLS, "allow"],
    ["edit", "deny"],
  ]);
  const middle = await written(false, { permission: { bash: "allow", "*": "ask", edit: "deny" } });
  assert.deepEqual(inOrder(middle.permission), [
    ["bash", "allow"],
    ["*", "ask"],
    [TOOLS, "allow"],
    ["edit", "deny"],
  ]);
  const last = await written(false, { permission: { bash: "allow", edit: "deny", "*": "ask" } });
  assert.deepEqual(inOrder(last.permission), [
    ["bash", "allow"],
    ["edit", "deny"],
    ["*", "ask"],
    [TOOLS, "allow"],
  ]);
});

test("the rule for Octoboard's tools goes first where the rules write no default", async () => {
  const config = await written(false, { permission: { bash: "allow", edit: "deny" } });
  assert.deepEqual(inOrder(config.permission), [
    [TOOLS, "allow"],
    ["bash", "allow"],
    ["edit", "deny"],
  ]);
  const none = await written(false, {});
  assert.deepEqual(inOrder(none.permission), [[TOOLS, "allow"]]);
});

test("a rule of the user's that names one of Octoboard's tools after their default still holds", async () => {
  const config = await written(false, {
    permission: { "*": "allow", octoboard_archive_session: "deny", "octoboard_*": "ask" },
  });
  assert.deepEqual(inOrder(config.permission), [
    ["*", "allow"],
    [TOOLS, "allow"],
    ["octoboard_archive_session", "deny"],
    ["octoboard_*", "ask"],
  ]);
  const noDefault = await written(true, { permission: { octoboard_archive_session: "deny" } });
  assert.deepEqual(inOrder(noDefault.permission), [
    ["*", "ask"],
    [TOOLS, "allow"],
    ["octoboard_archive_session", "deny"],
  ]);
});

test("a rule that names one of Octoboard's tools before the user's default stays overruled by it", async () => {
  const config = await written(false, { permission: { octoboard_report: "deny", "*": "ask", edit: "deny" } });
  assert.deepEqual(inOrder(config.permission), [
    ["octoboard_report", "deny"],
    ["*", "ask"],
    [TOOLS, "allow"],
    ["edit", "deny"],
  ]);
});

test("a rule of the user's under Octoboard's own key stands where it follows their default, and gives way before it", async () => {
  const after = await written(false, { permission: { "*": "ask", edit: "deny", [TOOLS]: "deny" } });
  assert.deepEqual(inOrder(after.permission), [
    ["*", "ask"],
    ["edit", "deny"],
    [TOOLS, "deny"],
  ]);
  const noDefault = await written(true, { permission: { [TOOLS]: "ask", edit: "deny" } });
  assert.deepEqual(inOrder(noDefault.permission), [
    ["*", "ask"],
    [TOOLS, "ask"],
    ["edit", "deny"],
  ]);
  const alone = await written(false, { permission: { edit: "deny", [TOOLS]: "deny" } });
  assert.deepEqual(inOrder(alone.permission), [
    ["edit", "deny"],
    [TOOLS, "deny"],
  ]);
  const before = await written(false, { permission: { [TOOLS]: "deny", "*": "ask", edit: "deny" } });
  assert.deepEqual(inOrder(before.permission), [
    ["*", "ask"],
    [TOOLS, "allow"],
    ["edit", "deny"],
  ]);
});

test("with no approval mode chosen the user's rules are left as they are", async () => {
  const rules = { bash: "allow", edit: "deny", "*": "allow" };
  const config = await written(false, { permission: { ...rules } });
  assert.deepEqual(inOrder(config.permission), [...Object.entries(rules), [TOOLS, "allow"]]);

  const one = await written(false, { permission: "deny" });
  assert.deepEqual(inOrder(one.permission), [
    ["*", "deny"],
    [TOOLS, "allow"],
  ]);
});

test("an agent's own rules stay the user's, with Octoboard's rule after the agent's own default only", async () => {
  const config = await written(true, {
    permission: { edit: "deny" },
    agent: {
      build: { permission: { "*": "allow", webfetch: "deny", octoboard_get_session: "deny" } },
      plan: { permission: "deny" },
      docs: { permission: { webfetch: "deny", octoboard_report: "ask" } },
      review: { model: "x" },
    },
  });
  assert.deepEqual(inOrder(config.permission), [
    ["*", "ask"],
    [TOOLS, "allow"],
    ["edit", "deny"],
  ]);
  assert.deepEqual(inOrder(config.agent.build.permission), [
    ["*", "allow"],
    [TOOLS, "allow"],
    ["webfetch", "deny"],
    ["octoboard_get_session", "deny"],
  ]);
  assert.deepEqual(inOrder(config.agent.plan.permission), [
    ["*", "deny"],
    [TOOLS, "allow"],
  ]);
  // No default of its own: nothing of Octoboard's is overruled there, and nothing is written.
  assert.deepEqual(config.agent.docs, { permission: { webfetch: "deny", octoboard_report: "ask" } });
  assert.deepEqual(config.agent.review, { model: "x" });
});

test("a permission that is neither an action nor rules is left alone", async () => {
  const config = await written(true, { permission: ["ask"], agent: { build: { permission: 7 } } });
  assert.deepEqual(config, { permission: ["ask"], agent: { build: { permission: 7 } } });
});

test("a failure midway through the writes puts back what was written, and throws nothing", async () => {
  // Only the agent cannot be assigned to: the configuration's own rules are written first.
  const rules = { edit: "deny" };
  const config = { permission: rules, agent: { build: Object.freeze({ permission: { "*": "ask" } }) } };
  await written(true, config);
  assert.equal(config.permission, rules);
  assert.deepEqual(config, { permission: { edit: "deny" }, agent: { build: { permission: { "*": "ask" } } } });

  const whole = Object.freeze({ permission: { edit: "deny" } });
  await written(true, whole);
  assert.deepEqual(whole, { permission: { edit: "deny" } });

  const { hooks } = await load(launch(true));
  await hooks.config(null);
  await hooks.config(undefined);
});

test("the hook writes no file", async () => {
  const { dir, hooks } = await load(launch(true));
  const before = fs.readdirSync(dir).sort();
  await hooks.config({ permission: { edit: "deny" } });
  assert.deepEqual(fs.readdirSync(dir).sort(), before);
  assert.deepEqual(before, ["octoboard.js", "octoboard.json", "package.json"]);
});

test("a data file with no rule to write leaves the permission untouched", async () => {
  const { hooks } = await load({ hook: "/nonexistent/hook" });
  const config = { permission: { edit: "deny" }, agent: { build: { permission: "ask" } } };
  await hooks.config(config);
  assert.deepEqual(config, { permission: { edit: "deny" }, agent: { build: { permission: "ask" } } });
});

test("without its data file the plugin offers no hook", async () => {
  const { hooks } = await load(null);
  assert.deepEqual(hooks, {});
});

// A session of the plugin's own, with what it forwards and reports from here on.
async function session(t, context = {}) {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  t.after(() => t.mock.timers.reset());
  const { hooks } = await load(launch(false), context);
  const from = { forwarded: forwarded.length, reported: reported.length };
  return {
    emit: (type, properties) => hooks.event({ event: { type, properties } }),
    // Lets every payload queued for the hook script reach it.
    async settle() {
      for (let round = 0; round < 50; round += 1) await new Promise((resolve) => setImmediate(resolve));
    },
    forwarded: () => forwarded.slice(from.forwarded),
    reported: () => reported.slice(from.reported),
    // A root session's turn from the prompt's submission to its start.
    async startTurn(sessionID, messageID, prompt) {
      await this.emit("message.updated", { info: { id: messageID, role: "user", sessionID } });
      await this.emit("message.part.updated", {
        part: { id: `${messageID}-text`, type: "text", text: prompt, messageID, sessionID },
      });
      await this.emit("session.status", { sessionID, status: { type: "busy" } });
    },
  };
}

const names = (payloads) => payloads.map((payload) => [payload.hook_event_name, payload.session_id]);

test("a turn's start and end are forwarded with the prompt and the last answer, and reported", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "do it");
  await s.emit("message.updated", { info: { id: "msg_2", role: "assistant", sessionID: "ses_a" } });
  await s.emit("message.part.updated", {
    part: { id: "part_2", type: "text", text: "Done.", messageID: "msg_2", sessionID: "ses_a" },
  });
  await s.emit("session.idle", { sessionID: "ses_a" });
  await s.settle();
  assert.deepEqual(s.forwarded(), [
    { format: 1, hook_event_name: "session.created", session_id: "ses_a" },
    { format: 1, hook_event_name: "turn.started", turn_id: "msg_1", prompt: "do it", session_id: "ses_a" },
    { format: 1, hook_event_name: "session.idle", turn_id: "msg_1", last_assistant_message: "Done.", session_id: "ses_a" },
  ]);
  assert.deepEqual(s.reported(), ["working", "idle"]);
});

test("a prompt is told of only once it has stayed open for the bound", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "run it");
  await s.emit("permission.asked", { sessionID: "ses_a", id: "per_1", permission: "bash" });
  t.mock.timers.tick(PROMPT_BOUND_MS - 1);
  await s.settle();
  assert.deepEqual(names(s.forwarded()), [
    ["session.created", "ses_a"],
    ["turn.started", "ses_a"],
  ]);
  assert.deepEqual(s.reported(), ["working"]);

  t.mock.timers.tick(1);
  await s.settle();
  assert.deepEqual(s.forwarded().at(-1), {
    format: 1,
    hook_event_name: "permission.asked",
    session_id: "ses_a",
    request_id: "per_1",
    tool: "bash",
  });
  assert.deepEqual(s.reported(), ["working", "blocked:kind=permission"]);

  await s.emit("permission.replied", { sessionID: "ses_a", requestID: "per_1" });
  await s.settle();
  assert.deepEqual(s.forwarded().at(-1), {
    format: 1,
    hook_event_name: "permission.replied",
    session_id: "ses_a",
    request_id: "per_1",
  });
  assert.deepEqual(s.reported(), ["working", "blocked:kind=permission", "working"]);
});

test("a prompt answered within the bound is never told of, and its answer is still forwarded", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "run it");
  await s.emit("permission.asked", { sessionID: "ses_a", id: "per_1", permission: "bash" });
  t.mock.timers.tick(20);
  await s.emit("permission.replied", { sessionID: "ses_a", requestID: "per_1" });
  t.mock.timers.tick(PROMPT_BOUND_MS * 2);
  await s.settle();
  assert.deepEqual(names(s.forwarded()), [
    ["session.created", "ses_a"],
    ["turn.started", "ses_a"],
    ["permission.replied", "ses_a"],
  ]);
  assert.deepEqual(s.reported(), ["working"]);
});

test("a question is told of as one, under a permission prompt while both are open", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "ask me");
  await s.emit("question.asked", { sessionID: "ses_a", id: "que_1" });
  t.mock.timers.tick(PROMPT_BOUND_MS);
  await s.emit("permission.asked", { sessionID: "ses_a", id: "per_1", permission: "edit" });
  t.mock.timers.tick(PROMPT_BOUND_MS);
  await s.settle();
  assert.deepEqual(
    s.forwarded().slice(2).map((payload) => [payload.hook_event_name, payload.request_id, payload.tool]),
    [
      ["question.asked", "que_1", "question"],
      ["permission.asked", "per_1", "edit"],
    ],
  );
  await s.emit("permission.replied", { sessionID: "ses_a", requestID: "per_1" });
  await s.settle();
  assert.deepEqual(s.reported(), ["working", "blocked:kind=question", "blocked:kind=permission", "blocked:kind=question"]);
});

test("a prompt is told of with its call and what it asks, a file's diff left out", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "edit it");
  await s.emit("permission.asked", {
    sessionID: "ses_a",
    id: "per_1",
    permission: "edit",
    patterns: ["made.txt"],
    metadata: { filepath: "/w/made.txt", diff: "Index: /w/made.txt\n+made\n" },
    always: ["*"],
    tool: { messageID: "msg_2", callID: "call_1" },
  });
  t.mock.timers.tick(PROMPT_BOUND_MS);
  await s.settle();
  assert.deepEqual(s.forwarded().at(-1), {
    format: 1,
    hook_event_name: "permission.asked",
    session_id: "ses_a",
    request_id: "per_1",
    tool: "edit",
    call_id: "call_1",
    input: { patterns: ["made.txt"], always: ["*"], filepath: "/w/made.txt" },
  });
  await s.emit("permission.replied", { sessionID: "ses_a", requestID: "per_1" });
  await s.emit("question.asked", {
    sessionID: "ses_a",
    id: "que_1",
    questions: [
      {
        question: "Which?",
        header: "Which",
        multiple: true,
        options: [{ label: "A", description: "a" }, { label: "B" }, { description: "no label" }],
      },
      { header: "no question" },
    ],
    tool: { messageID: "msg_2", callID: "call_2" },
  });
  t.mock.timers.tick(PROMPT_BOUND_MS);
  await s.settle();
  const asked = JSON.parse(JSON.stringify(s.forwarded().at(-1)));
  assert.deepEqual(asked, {
    format: 1,
    hook_event_name: "question.asked",
    session_id: "ses_a",
    request_id: "que_1",
    tool: "question",
    call_id: "call_2",
    input: {
      questions: [
        { question: "Which?", header: "Which", multiple: true, options: [{ label: "A", description: "a" }, { label: "B" }] },
      ],
    },
  });
});

test("an edit's file is forwarded as its prompt draws it, against the session's directory", async (t) => {
  const s = await session(t, { directory: "/w/project" });
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "edit them");
  const home = homedir();
  const files = [
    ["/w/project/sub/made.txt", join("sub", "made.txt")],
    ["sub/made.txt", join("sub", "made.txt")],
    ["/w/project", "."],
    ["/w/other/made.txt", "/w/other/made.txt"],
    [join(home, "notes", "made.txt"), "~" + sep + join("notes", "made.txt")],
    [home, "~"],
  ];
  for (const [n, [filepath, drawn]] of files.entries()) {
    await s.emit("permission.asked", {
      sessionID: "ses_a",
      id: `per_${n}`,
      permission: "edit",
      patterns: [filepath],
      metadata: { filepath },
      always: ["*"],
    });
    t.mock.timers.tick(PROMPT_BOUND_MS);
    await s.settle();
    assert.equal(s.forwarded().at(-1).input.drawn_path, drawn, filepath);
    await s.emit("permission.replied", { sessionID: "ses_a", requestID: `per_${n}` });
  }
  // Only an edit's prompt draws its file so.
  await s.emit("permission.asked", {
    sessionID: "ses_a",
    id: "per_read",
    permission: "external_directory",
    patterns: ["/etc/*"],
    metadata: { filepath: "/etc/hosts" },
  });
  t.mock.timers.tick(PROMPT_BOUND_MS);
  await s.settle();
  assert.equal(s.forwarded().at(-1).input.drawn_path, undefined);
});

test("the events of a conversation the user has left are not forwarded", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "long work");
  // A prompt is submitted in another conversation: that one is the one shown from here on.
  await s.emit("session.created", { info: { id: "ses_b" } });
  await s.startTurn("ses_b", "msg_2", "other work");
  await s.settle();
  const shown = s.forwarded().length;
  const reports = s.reported().length;

  await s.emit("permission.asked", { sessionID: "ses_a", id: "per_1", permission: "bash" });
  t.mock.timers.tick(PROMPT_BOUND_MS * 2);
  await s.emit("permission.replied", { sessionID: "ses_a", requestID: "per_1" });
  await s.emit("session.idle", { sessionID: "ses_a" });
  await s.settle();
  assert.deepEqual(s.forwarded().slice(shown), []);
  assert.deepEqual(s.reported().slice(reports), []);

  await s.emit("session.idle", { sessionID: "ses_b" });
  await s.settle();
  assert.deepEqual(names(s.forwarded().slice(shown)), [["session.idle", "ses_b"]]);
  assert.deepEqual(s.reported().slice(reports), ["idle"]);
});

test("an answer that comes once the turn is over is dropped", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "run it");
  await s.emit("permission.asked", { sessionID: "ses_a", id: "per_1", permission: "bash" });
  t.mock.timers.tick(PROMPT_BOUND_MS);
  await s.emit("session.idle", { sessionID: "ses_a" });
  await s.settle();
  const ended = s.forwarded().length;
  const reports = s.reported();
  assert.equal(reports.at(-1), "idle");

  await s.emit("permission.replied", { sessionID: "ses_a", requestID: "per_1" });
  await s.emit("question.rejected", { sessionID: "ses_a", requestID: "que_9" });
  t.mock.timers.tick(PROMPT_BOUND_MS * 2);
  await s.settle();
  assert.equal(s.forwarded().length, ended);
  assert.deepEqual(s.reported(), reports);
});

test("a subagent's prompt is the root session's own wait", async (t) => {
  const s = await session(t);
  await s.emit("session.created", { info: { id: "ses_a" } });
  await s.startTurn("ses_a", "msg_1", "delegate");
  await s.emit("session.created", { info: { id: "ses_child", parentID: "ses_a" } });
  await s.emit("permission.asked", { sessionID: "ses_child", id: "per_1", permission: "bash" });
  t.mock.timers.tick(PROMPT_BOUND_MS);
  await s.settle();
  assert.deepEqual(s.forwarded().at(-1), {
    format: 1,
    hook_event_name: "permission.asked",
    session_id: "ses_a",
    request_id: "per_1",
    tool: "bash",
  });
  assert.deepEqual(s.forwarded()[2], {
    format: 1,
    hook_event_name: "session.created",
    session_id: "ses_child",
    subagent: true,
  });
  assert.equal(s.reported().at(-1), "blocked:kind=permission");
});

// The cross-project guard.

const guardedLaunch = (extra = {}) => ({
  ...launch(false),
  guard: GUARD,
  guard_tools: ["write", "edit", "apply_patch"],
  guard_timeout_ms: 10000,
  ...extra,
});
const REFUSAL = JSON.stringify({ decision: "deny", reason: "Blocked: this file is in project \"other\", not yours." });

test("a call of a tool that writes a file is refused with the guard's reason", async () => {
  const { hooks } = await load(guardedLaunch(), { directory: "/work/project" });
  guard.answer = `${REFUSAL}\n`;
  guarded.length = 0;
  const args = { filePath: "/other/made.txt", content: "made\n" };
  await assert.rejects(hooks["tool.execute.before"]({ tool: "write", sessionID: "ses_a", callID: "c1" }, { args }), {
    message: 'Blocked: this file is in project "other", not yours.',
  });
  assert.deepEqual(guarded, [{ format: 1, tool: "write", args, cwd: "/work/project" }]);
});

test("a call the guard prints nothing for, or anything but a refusal, goes through", async () => {
  const { hooks } = await load(guardedLaunch());
  for (const answer of ["", "not json", JSON.stringify({ decision: "allow", reason: "x" }), JSON.stringify({ decision: "deny" })]) {
    guard.answer = answer;
    await hooks["tool.execute.before"]({ tool: "edit" }, { args: { filePath: "a" } });
  }
});

test("only the tools the launch names are asked about, and none without a guard", async () => {
  guard.answer = REFUSAL;
  guarded.length = 0;
  const { hooks } = await load(guardedLaunch());
  await hooks["tool.execute.before"]({ tool: "bash" }, { args: { command: "echo > /other/x" } });
  await hooks["tool.execute.before"]({ tool: "read" }, { args: { filePath: "/other/x" } });
  assert.equal(guarded.length, 0);
  const unguarded = await load(guardedLaunch({ guard: null }));
  await unguarded.hooks["tool.execute.before"]({ tool: "write" }, { args: { filePath: "/other/x" } });
  assert.equal(guarded.length, 0);
});

test("a guard that cannot start, or outlasts its deadline, lets the call through", async () => {
  guard.answer = REFUSAL;
  const { hooks } = await load(guardedLaunch({ guard_timeout_ms: 20 }));
  guard.starts = false;
  try {
    await hooks["tool.execute.before"]({ tool: "write" }, { args: { filePath: "/other/x" } });
  } finally {
    guard.starts = true;
  }
  guard.close = false;
  try {
    await hooks["tool.execute.before"]({ tool: "write" }, { args: { filePath: "/other/x" } });
  } finally {
    guard.close = true;
  }
});

// Copies `files` (a map of a name in the scratch directory to a source beside this test) into a
// scratch directory of their own, as the daemon writes them, and returns the directory.
function scratchWith(files) {
  const dir = fs.mkdtempSync(join(tmpdir(), "octoboard-opencode-entry-"));
  directories.push(dir);
  fs.writeFileSync(join(dir, "package.json"), JSON.stringify({ type: "module" }));
  for (const [name, source] of Object.entries(files)) {
    fs.mkdirSync(dirname(join(dir, name)), { recursive: true });
    fs.copyFileSync(join(dirname(PLUGIN), source), join(dir, name));
  }
  return dir;
}

test("a failure while the plugin sets itself up turns it off instead of failing OpenCode", async () => {
  const context = {
    get directory() {
      throw new Error("a changed plugin interface");
    },
  };
  const { hooks } = await load(launch(false), context);
  assert.deepEqual(hooks, {});
});

test("the second generation's plugin turns itself off where OpenCode's plugin interface changed", async () => {
  const dir = scratchWith({ "octoboard.js": "plugin.js", "plugin/index.js": "plugin-v2.js" });
  fs.writeFileSync(join(dir, "octoboard.json"), JSON.stringify(launch(false)));
  const plugin = (await import(pathToFileURL(join(dir, "plugin", "index.js")).href)).default;
  const changed = {
    location: { directory: dir },
    tool: {
      hook: () => {
        throw new Error("a changed plugin interface");
      },
    },
  };
  const dispose = await plugin.setup(changed);
  assert.equal(typeof dispose, "function");
  dispose();
  assert.equal(typeof (await plugin.setup({})), "function");
});

test("the TUI plugins turn themselves off where the route cannot be read, and leave nothing running", async () => {
  const dir = scratchWith({ "tui.mjs": "tui.mjs", "tui-v2.js": "tui-v2.js" });
  const v1 = (await import(pathToFileURL(join(dir, "tui.mjs")).href)).default;
  const v2 = (await import(pathToFileURL(join(dir, "tui-v2.js")).href)).default;
  await v1.tui({});
  await v1.tui({ route: {} });
  const dispose = v2.setup({});
  assert.equal(typeof dispose, "function");
  dispose();
  // A timer left running would keep this test's process alive past the suite.
  assert.ok(!fs.existsSync(join(dir, "attached-session")));
});
