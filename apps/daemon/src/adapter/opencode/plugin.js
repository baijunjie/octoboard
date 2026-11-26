// Octoboard's plugin for OpenCode. OpenCode has no hooks, so this module, loaded into its process
// for one session, forwards a small fixed set of its bus events to Octoboard through the
// session's hook script, as payloads in Octoboard's own format (`hooks.rs` beside this file reads
// them). It is written into the session's scratch directory at launch beside `octoboard.json`,
// which holds what differs per session.
//
// It only forwards: no network call of its own, nothing awaited on the event path, nothing
// drawn on the screen, and no error thrown into OpenCode but one. A daemon that cannot be reached
// is silent.
//
// The one error it throws is the cross-project guard's refusal. Where the launch has a guard, it
// runs the guard's script for every call of a tool that writes a file, before the call runs, and
// throws the refusal's reason from `tool.execute.before`: OpenCode then refuses the call and hands
// the model that reason as the tool's result (1.19.6). A guard that cannot decide lets the call
// through: one that fails, prints nothing, or outlasts its deadline.
//
// The one thing it writes to the terminal is whether a turn runs, as a program status report
// (OSC 7501), which draws nothing and which Octoboard's terminal takes out of the output:
// OpenCode's title says the same during a turn as after it, and Octoboard presses an interrupt
// key only where the terminal shows the turn running as well as the events.
//
// One terminal can hold several conversations (`/new`, the session list), and no event says which
// one it shows. A prompt can only be submitted in the one shown, so the conversation a prompt was
// last submitted in is taken for it: its events are the session's, and those of a conversation
// the user has left (a turn still running there, a permission prompt it raises, which OpenCode
// does not draw until the user is back) are not forwarded.

import { spawn } from "node:child_process";
import { readFileSync, writeSync } from "node:fs";
import { request } from "node:http";
import { homedir } from "node:os";
import { isAbsolute, relative, resolve, sep } from "node:path";

// The payload format, which the interpreter reads tolerantly; raised when a field changes meaning.
const FORMAT = 1;
// How long one run of the hook script may hold the queue. The script has a deadline of its own,
// far shorter; this only keeps a script that never exits from holding every later event.
const HOOK_DEADLINE_MS = 15000;
// The most bytes of the guard's answer read: a refusal is a few hundred.
const GUARD_ANSWER_LIMIT = 64 * 1024;
// The longest text forwarded in one payload, in UTF-16 units.
const TEXT_LIMIT = 200000;
// How long a permission prompt or a question has to stay open before anyone is told the session
// waits. Under `--auto` OpenCode asks and answers a permission itself within the same moment (6
// to 24 ms on 1.19.6), and nobody is wanted for that.
const PROMPT_BOUND_MS = 300;

function readData() {
  try {
    const data = JSON.parse(readFileSync(new URL("./octoboard.json", import.meta.url), "utf8"));
    return data && typeof data.hook === "string" ? data : null;
  } catch {
    return null;
  }
}

// Reports the state of the session's turn to the terminal, as one short write. Where the
// terminal takes none or only part of it (standard output can be non-blocking with its buffer
// full), the rest is tried once more; what is still left then is dropped, and the next report
// tells the state again.
function report(state) {
  if (process.env.OCTOBOARD_OPENCODE_SERVER) return;
  const bytes = Buffer.from(`\u001b]7501;state=${state}\u001b\\`, "latin1");
  let written = 0;
  for (let attempt = 0; attempt < 2 && written < bytes.length; attempt += 1) {
    try {
      written += writeSync(1, bytes, written, bytes.length - written);
    } catch (error) {
      if (!error || error.code !== "EAGAIN") return;
    }
  }
}

// `text` no longer than the limit, never ending in half of a character outside the basic plane:
// a lone surrogate is one the daemon's JSON reader refuses, and the whole payload with it.
function clip(text) {
  if (typeof text !== "string" || text.length <= TEXT_LIMIT) return text;
  const last = text.charCodeAt(TEXT_LIMIT - 1);
  return text.slice(0, last >= 0xd800 && last <= 0xdbff ? TEXT_LIMIT - 1 : TEXT_LIMIT);
}

// A file as OpenCode 1.19.6 draws it in an `edit` permission prompt (`→ Edit <file>`): relative
// to the session's `directory` where it lies inside it (`.` for the directory itself), else under
// `~` where it lies inside `home`, else whole.
function drawnPath(file, directory, home) {
  const path = isAbsolute(file) ? file : resolve(directory, file);
  const inside = relative(directory, path);
  if (!inside) return ".";
  if (inside !== ".." && !inside.startsWith(".." + sep)) return inside;
  if (!home) return path;
  const underHome = relative(home, path);
  if (underHome === "") return "~";
  if (underHome === ".." || underHome.startsWith(".." + sep) || isAbsolute(underHome)) return path;
  return "~" + sep + underHome;
}

// What a permission prompt or a question asks, from its event's properties (1.19.6): a
// permission's `patterns`, the `always` patterns its `Allow always` would allow, and its
// `metadata` (`command` for `bash`, `filepath` for `edit`) without the `diff`, which can be a
// whole file, with, for an `edit` in a session of a known `directory`, its file as the prompt
// draws it (`drawn_path`, [`drawnPath`]); a question's `questions`, each with its `question`,
// `header`, `options` (`label`, `description`) and `multiple`. Strings are clipped; `null` where
// the event has none of it.
function askedInput(kind, properties, directory) {
  const strings = (value) =>
    typeof value === "string" ? clip(value) : Array.isArray(value) ? value.map(strings) : value;
  if (kind === "question") {
    if (!Array.isArray(properties.questions)) return null;
    const questions = properties.questions
      .filter((asked) => asked && typeof asked.question === "string")
      .map((asked) => ({
        question: clip(asked.question),
        header: typeof asked.header === "string" ? clip(asked.header) : undefined,
        multiple: asked.multiple === true,
        options: (Array.isArray(asked.options) ? asked.options : [])
          .filter((option) => option && typeof option.label === "string")
          .map((option) => ({
            label: clip(option.label),
            description: typeof option.description === "string" ? clip(option.description) : undefined,
          })),
      }));
    return { questions };
  }
  const input = {};
  if (Array.isArray(properties.patterns)) input.patterns = strings(properties.patterns.filter((p) => typeof p === "string"));
  if (Array.isArray(properties.always)) input.always = strings(properties.always.filter((p) => typeof p === "string"));
  const metadata = properties.metadata;
  if (metadata && typeof metadata === "object" && !Array.isArray(metadata)) {
    for (const [key, value] of Object.entries(metadata)) {
      if (key !== "diff" && typeof value === "string") input[key] = clip(value);
    }
    if (properties.permission === "edit" && directory && typeof metadata.filepath === "string" && metadata.filepath) {
      input.drawn_path = clip(drawnPath(metadata.filepath, directory, homedir()));
    }
  }
  return Object.keys(input).length ? input : null;
}

// The rules of a `permission` as an object of its own, in the order they are written, which is
// the order OpenCode follows them in: of the rules that match a tool the last one holds. One action
// for every tool is the rule for `*`. `null` for a value that is neither, which is left alone.
function rulesOf(permission) {
  if (permission === undefined || permission === null) return {};
  if (typeof permission === "string") return { "*": permission };
  if (typeof permission === "object" && !Array.isArray(permission)) return { ...permission };
  return null;
}

// `rules` with `action` as their default: the rule for `*`, which holds for a tool only where no
// rule written after it matches. Where the rules have a `*` of their own the action takes its
// place there, so the rules written after it still hold and those before it stay overruled, as
// they were by that default; where they have none it goes first, beneath every rule. A `*` that
// holds rules by pattern gets the default among those the same way.
function withDefault(rules, action) {
  if (!Object.hasOwn(rules, "*")) return { "*": action, ...rules };
  const own = rules["*"];
  rules["*"] = own && typeof own === "object" && !Array.isArray(own) ? withDefault({ ...own }, action) : action;
  return rules;
}

// `rules` with `action` for `pattern` right after their `*`, or first where they have none: it
// holds over the default, which names no tool, and every rule written after the default holds
// over it, so a rule of the user's that names one of the tools the pattern matches still wins. A
// rule under the pattern's own key that is written there already (after the `*`, or anywhere
// where there is none) is the user's and stands as written; one written before the `*` was
// overruled by it, and gives way.
function withAfterDefault(rules, pattern, action) {
  const keys = Object.keys(rules);
  const star = keys.indexOf("*");
  if (keys.indexOf(pattern) > star) return rules;
  const written = {};
  if (star === -1) written[pattern] = action;
  for (const key of keys) {
    if (key === pattern) continue;
    written[key] = rules[key];
    if (key === "*") written[pattern] = action;
  }
  return written;
}

// Writes the launch's permission rules into the configuration OpenCode has merged, in memory
// only: under the `ask` approval mode asking as the default beneath the user's rules, and the
// rule that allows Octoboard's own tools right after the default, so that the default does not
// ask for them and a rule of the user's that names one of them still holds. An agent's own
// `permission` is followed after the configuration's, so where it writes a `*` of its own the
// rule for Octoboard's tools goes after that one too; an agent's rules with no `*` overrule
// nothing of Octoboard's and are left alone, and so are an agent's other rules, its default
// included. Everything is worked out before anything is assigned, and what was assigned is put
// back where a later assignment fails, so a failure leaves the configuration as OpenCode merged
// it.
function writeRules(config, data) {
  const tools = typeof data.tools_rule === "string" && data.tools_rule ? data.tools_rule : null;
  const action = typeof data.default_action === "string" && data.default_action ? data.default_action : null;
  const changes = [];
  let rules = rulesOf(config.permission);
  if (rules) {
    if (action) rules = withDefault(rules, action);
    if (tools) rules = withAfterDefault(rules, tools, "allow");
    if (action || tools) changes.push([config, rules]);
  }
  const agents = config.agent && typeof config.agent === "object" ? Object.values(config.agent) : [];
  for (const agent of agents) {
    if (!tools || !agent || typeof agent !== "object") continue;
    const own = rulesOf(agent.permission);
    if (own && Object.hasOwn(own, "*")) changes.push([agent, withAfterDefault(own, tools, "allow")]);
  }
  const assigned = [];
  try {
    for (const [holder, written] of changes) {
      const before = holder.permission;
      holder.permission = written;
      assigned.push([holder, before]);
    }
  } catch (error) {
    for (const [holder, before] of assigned.reverse()) holder.permission = before;
    throw error;
  }
}

// Runs `script` with `body` on its standard input and resolves to what it printed, or to `null`
// where it could not be run, failed to start, or outlasted `deadline` milliseconds (it is killed
// then). The script is a batch file on Windows, which only a shell runs, quoted for a path with
// spaces.
function runScript(script, body, deadline) {
  return new Promise((resolve) => {
    let timer;
    let settled = false;
    const chunks = [];
    let size = 0;
    const done = (value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(value);
    };
    try {
      const windows = process.platform === "win32";
      const child = spawn(windows ? `"${script}"` : script, [], {
        stdio: ["pipe", "pipe", "ignore"],
        windowsHide: true,
        shell: windows,
      });
      timer = setTimeout(() => {
        try {
          child.kill();
        } catch {}
        done(null);
      }, deadline);
      child.stdout.on("data", (chunk) => {
        if (size >= GUARD_ANSWER_LIMIT) return;
        chunks.push(chunk);
        size += chunk.length;
      });
      child.on("error", () => done(null));
      child.on("close", () => done(Buffer.concat(chunks).toString("utf8")));
      child.stdin.on("error", () => {});
      child.stdin.end(body);
    } catch {
      done(null);
    }
  });
}

// The reason the guard refuses a call for, from what its script printed: a decision of `deny`
// with a reason (`hooks.rs`, `deny_edit`). `null` for anything else, which lets the call through.
function refusalOf(answer) {
  if (typeof answer !== "string" || !answer.trim()) return null;
  try {
    const decision = JSON.parse(answer);
    if (decision && decision.decision === "deny" && typeof decision.reason === "string" && decision.reason) {
      return decision.reason;
    }
  } catch {}
  return null;
}

// The attached TUI has no --auto flag. Answer the same permission event the standalone TUI
// answers in that mode. The server emits this event only for a permission its rules let it ask.
function approveServerPermission(server, directory, id) {
  const password = process.env.OPENCODE_SERVER_PASSWORD;
  if (!password || server?.protocol !== "http:" || server.hostname !== "127.0.0.1" || !server.port) return;
  const route = new URL(`/permission/${encodeURIComponent(id)}/reply`, server);
  if (directory) route.searchParams.set("directory", directory);
  const body = JSON.stringify({ reply: "once" });
  const approval = request(route, {
    method: "POST",
    agent: false,
    auth: `opencode:${password}`,
    timeout: 3000,
    headers: { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(body) },
  }, (answer) => answer.resume());
  approval.on("error", () => {});
  approval.on("timeout", () => approval.destroy());
  approval.end(body);
}

// OpenCode's entry point, total: whatever fails while the plugin sets itself up turns Octoboard's
// plugin off, with no hook of its own, and leaves OpenCode running. OpenCode goes on past a plugin
// that throws as it loads too, logging the error to its log file and showing nothing (1.19.6,
// `testdata/opencode-captures.md`, "A plugin that throws while loading"); this keeps the outcome
// the same whatever a release does with such a plugin, and the second generation's bridge, which
// calls it, gets the same. The daemon then tells the session that no events arrive.
export const OctoboardPlugin = async (context, v2 = false) => {
  try {
    return await setUp(context, v2);
  } catch {
    return {};
  }
};

const setUp = async (context, v2) => {
  const data = readData();
  if (!data) return {};
  const serverAuto = v2 !== true && process.env.OCTOBOARD_OPENCODE_SERVER === "auto";
  const attachedRoute = process.env.OCTOBOARD_OPENCODE_ROUTE;
  const directory = context && typeof context.directory === "string" ? context.directory : null;
  const guard = typeof data.guard === "string" && data.guard ? data.guard : null;
  const guardTools = new Set(Array.isArray(data.guard_tools) ? data.guard_tools : []);
  const guardDeadline =
    typeof data.guard_timeout_ms === "number" && data.guard_timeout_ms > 0 ? data.guard_timeout_ms : 10000;

  // One run of the hook script at a time, in the order the events came: each run is a process of
  // its own, and two racing to the daemon could arrive swapped (an idle before the reply that
  // preceded it would leave the session shown working).
  let queue = Promise.resolve();
  const forward = (payload) => {
    const body = JSON.stringify({ format: FORMAT, ...payload });
    queue = queue.then(
      () =>
        new Promise((resolve) => {
          let timer;
          const done = () => {
            clearTimeout(timer);
            resolve();
          };
          try {
            // The script is a batch file on Windows, which only a shell runs, quoted for a path
            // with spaces.
            const windows = process.platform === "win32";
            const child = spawn(windows ? `"${data.hook}"` : data.hook, [], {
              stdio: ["pipe", "ignore", "ignore"],
              windowsHide: true,
              shell: windows,
            });
            timer = setTimeout(() => {
              try {
                child.kill();
              } catch {}
              done();
            }, HOOK_DEADLINE_MS);
            child.on("error", done);
            child.on("close", done);
            child.stdin.on("error", () => {});
            child.stdin.end(body);
          } catch {
            done();
          }
        }),
    );
  };

  // Each session's parent, `null` for a root session: a child session is a subagent's.
  const parents = new Map();
  // What is known of each session's turn in progress.
  const turns = new Map();
  // Each message's role and session, for telling a prompt's text from an answer's.
  const roles = new Map();
  // The permission prompts and questions still open: the root session each waits in, its kind,
  // whether it has stayed open long enough to be told of, and the timer that tells of it.
  const waiting = new Map();
  // The root session the last prompt was submitted in: the conversation the terminal shows, as
  // far as an event can say.
  let current = null;

  const newTurn = () => ({
    open: false,
    promptMessage: null,
    prompt: null,
    // A user message that came while the turn ran, which starts the next turn if OpenCode had
    // not taken it into this one.
    nextMessage: null,
    nextPrompt: null,
    returned: false,
    answers: new Map(),
  });
  const turnOf = (sessionID) => {
    let turn = turns.get(sessionID);
    if (!turn) {
      turn = newTurn();
      turns.set(sessionID, turn);
    }
    return turn;
  };
  // Forgets a session's turn that is over, and its messages' roles: not another session's, whose
  // turn may still run (a subagent beside this one). A message that came while the turn ran is
  // kept as the one the next turn starts from.
  const endTurn = (sessionID, turn) => {
    turns.delete(sessionID);
    for (const [message, known] of roles) {
      if (known.sessionID === sessionID && message !== turn.nextMessage) roles.delete(message);
    }
    if (turn.nextMessage) {
      const next = newTurn();
      next.promptMessage = turn.nextMessage;
      next.prompt = turn.nextPrompt;
      turns.set(sessionID, next);
    }
  };
  // Tells the terminal what the session's turn waits on: a prompt told of and still open, or
  // nothing. OpenCode draws one prompt at a time and a permission prompt ahead of a question.
  const reportWaiting = () => {
    const kinds = Array.from(waiting.values())
      .filter((prompt) => prompt.told && prompt.root === current)
      .map((prompt) => prompt.kind);
    if (kinds.length === 0) report("working");
    else if (kinds.includes("permission")) report("blocked:kind=permission");
    else report("blocked:kind=question");
  };
  // Forgets the prompts of a root session whose turn is over.
  const forgetPrompts = (root) => {
    for (const [id, prompt] of waiting) {
      if (prompt.root !== root) continue;
      clearTimeout(prompt.timer);
      waiting.delete(id);
    }
  };
  const rootOf = (sessionID) => {
    let id = sessionID;
    for (let depth = 0; depth < 16; depth += 1) {
      const parent = parents.get(id);
      if (!parent) return id;
      id = parent;
    }
    return id;
  };
  // Whether the session, or the root session of a subagent's, is the conversation the terminal
  // shows.
  const shown = (sessionID) => {
    if (attachedRoute) {
      try {
        const selected = readFileSync(attachedRoute, "utf8");
        if (selected) return rootOf(sessionID) === rootOf(selected);
      } catch {}
    }
    return rootOf(sessionID) === current;
  };
  // A root session's turn starts, or the user is back in a turn still running: the conversation
  // is the one shown from here on.
  const startTurn = (sessionID, turn, prompt) => {
    const payload = { hook_event_name: "turn.started" };
    if (turn.promptMessage) payload.turn_id = turn.promptMessage;
    if (prompt !== null) payload.prompt = clip(prompt);
    if (!parents.get(sessionID)) current = sessionID;
    if (!shown(sessionID)) return;
    forward(marked(sessionID, payload));
    if (!parents.get(sessionID)) reportWaiting();
  };
  const marked = (sessionID, payload) =>
    parents.get(sessionID) ? { ...payload, session_id: sessionID, subagent: true } : { ...payload, session_id: sessionID };
  const noteSession = (info) => {
    if (info && typeof info.id === "string") parents.set(info.id, info.parentID || null);
  };
  // The text of the last answer of the turn that has any: a turn's earlier messages are its tool
  // calls, and a final message can be one too.
  const lastAnswer = (turn) => {
    let text = null;
    for (const parts of turn.answers.values()) {
      const joined = Array.from(parts.values()).join("").trim();
      if (joined) text = joined;
    }
    return text;
  };

  const observe = (event) => {
    const properties = event.properties || {};
    switch (event.type) {
      case "session.created": {
        noteSession(properties.info);
        const id = properties.info && properties.info.id;
        if (typeof id === "string") forward(marked(id, { hook_event_name: "session.created" }));
        return;
      }
      case "session.updated":
        noteSession(properties.info);
        return;
      case "message.updated": {
        const info = properties.info;
        if (!info || typeof info.id !== "string") return;
        const previous = roles.get(info.id);
        const completed = info.role === "assistant" && typeof info.time?.completed === "number";
        roles.set(info.id, { role: info.role, sessionID: info.sessionID, completed });
        if (completed && !previous?.completed && !parents.get(info.sessionID) && shown(info.sessionID)) {
          forward(marked(info.sessionID, { hook_event_name: "record.changed" }));
        }
        if (info.role === "user" && typeof info.sessionID === "string") {
          const turn = turnOf(info.sessionID);
          if (!turn.open) {
            if (turn.promptMessage !== info.id) {
              turn.promptMessage = info.id;
              turn.prompt = null;
            }
          } else if (turn.promptMessage !== info.id && turn.nextMessage !== info.id) {
            // A prompt sent while a turn runs joins that turn: OpenCode reports one idle for
            // both. One that comes as the turn ends starts the next instead, so it is kept.
            turn.nextMessage = info.id;
            turn.nextPrompt = null;
            // Submitted in a conversation the user had left: they are back in it, which is told
            // once the prompt's text is known.
            turn.returned = !parents.get(info.sessionID) && !shown(info.sessionID);
          }
        }
        return;
      }
      case "message.part.updated": {
        const part = properties.part;
        if (!part || part.type !== "text" || typeof part.text !== "string") return;
        const turn = turnOf(part.sessionID);
        const known = roles.get(part.messageID);
        const role = known && known.role;
        if (role === "user") {
          // The first text of the message is what was submitted; later ones are what OpenCode
          // attaches to it.
          if (part.synthetic) return;
          if (part.messageID === turn.promptMessage) {
            if (turn.prompt === null) turn.prompt = part.text;
          } else if (part.messageID === turn.nextMessage && turn.nextPrompt === null) {
            turn.nextPrompt = part.text;
            if (turn.returned) {
              turn.returned = false;
              startTurn(part.sessionID, turn, part.text);
            }
          }
        } else if (role === "assistant" && turn.open) {
          let parts = turn.answers.get(part.messageID);
          if (!parts) {
            parts = new Map();
            turn.answers.set(part.messageID, parts);
          }
          parts.set(part.id, part.text);
        }
        return;
      }
      case "session.status": {
        const sessionID = properties.sessionID;
        if (typeof sessionID !== "string" || !properties.status || properties.status.type !== "busy") return;
        const turn = turnOf(sessionID);
        // Busy repeats with every step of a turn; only the first one starts it.
        if (turn.open) return;
        turn.open = true;
        turn.answers = new Map();
        startTurn(sessionID, turn, turn.prompt);
        return;
      }
      case "session.idle": {
        const sessionID = properties.sessionID;
        if (typeof sessionID !== "string") return;
        const turn = turnOf(sessionID);
        // An idle with no turn open is the one that follows an error, or a repeat.
        if (!turn.open) return;
        const payload = { hook_event_name: "session.idle" };
        if (turn.promptMessage) payload.turn_id = turn.promptMessage;
        const answer = lastAnswer(turn);
        if (answer) payload.last_assistant_message = clip(answer);
        endTurn(sessionID, turn);
        if (!parents.get(sessionID)) forgetPrompts(sessionID);
        if (!shown(sessionID)) return;
        forward(marked(sessionID, payload));
        if (!parents.get(sessionID)) report("idle");
        return;
      }
      case "session.error": {
        const sessionID = properties.sessionID;
        if (typeof sessionID !== "string") return;
        const turn = turnOf(sessionID);
        if (!turn.open) return;
        const error = properties.error || {};
        // An interrupt is reported as an error of this name; anything else is a failure.
        const cancelled = error.name === "MessageAbortedError";
        const payload = { hook_event_name: cancelled ? "turn.cancelled" : "session.error" };
        if (turn.promptMessage) payload.turn_id = turn.promptMessage;
        if (!cancelled) {
          const message = error.data && typeof error.data.message === "string" ? error.data.message : error.name;
          if (typeof message === "string" && message) payload.error = clip(message);
        }
        endTurn(sessionID, turn);
        if (!parents.get(sessionID)) forgetPrompts(sessionID);
        if (!shown(sessionID)) return;
        forward(marked(sessionID, payload));
        if (!parents.get(sessionID)) report("idle");
        return;
      }
      case "permission.asked":
      case "question.asked": {
        if (typeof properties.sessionID !== "string" || typeof properties.id !== "string") return;
        // A subagent's prompt is drawn in the same terminal and waits on the same user, so it is
        // the session's own wait, under the session's own id.
        if (serverAuto && event.type === "permission.asked") {
          approveServerPermission(context.serverUrl, directory, properties.id);
        }
        const root = rootOf(properties.sessionID);
        const id = properties.id;
        const prompt = { root, kind: event.type === "question.asked" ? "question" : "permission", told: false, timer: null };
        const payload = {
          hook_event_name: event.type,
          session_id: root,
          request_id: id,
          tool: prompt.kind === "question" ? "question" : properties.permission,
        };
        // What the prompt asks, for Octoboard to tell it on the screen and answer it: the call it
        // is about, and for a permission its patterns and what the call does (a file's diff left
        // out), for a question the questions themselves.
        const call = properties.tool && properties.tool.callID;
        if (typeof call === "string" && call) payload.call_id = call;
        const input = askedInput(prompt.kind, properties, directory);
        if (input) payload.input = input;
        waiting.set(id, prompt);
        // Told only once it has stayed open, and only while it is the shown conversation's.
        prompt.timer = setTimeout(() => {
          if (waiting.get(id) !== prompt || !shown(root)) return;
          prompt.told = true;
          forward(payload);
          reportWaiting();
        }, PROMPT_BOUND_MS);
        if (typeof prompt.timer.unref === "function") prompt.timer.unref();
        return;
      }
      case "permission.replied":
      case "question.replied":
      case "question.rejected": {
        if (typeof properties.sessionID !== "string" || typeof properties.requestID !== "string") return;
        const prompt = waiting.get(properties.requestID);
        if (prompt) clearTimeout(prompt.timer);
        waiting.delete(properties.requestID);
        const root = rootOf(properties.sessionID);
        const turn = turns.get(root);
        // An answer that comes once the session's turn is over says nothing: forwarded, it would
        // put the session back at work with no turn running. (1.19.6 answers a prompt before the
        // idle when Esc rejects it, and not at all when the turn is aborted under it.)
        if (!turn || !turn.open || !shown(root)) return;
        // The answer is forwarded whether or not the prompt was told of: it says the turn works.
        forward({
          hook_event_name: event.type,
          session_id: root,
          request_id: properties.requestID,
        });
        // Another prompt can still be open: the turn waits until the last one is answered.
        if (prompt && prompt.told) reportWaiting();
        return;
      }
      default:
    }
  };

  // Asks the cross-project guard about a call of a tool that writes a file, and throws its
  // refusal. Nothing else is thrown: a failure of the guard, or of this function, lets the call
  // through.
  const checkEdit = async (input, output) => {
    if (!guard || !input || !guardTools.has(input.tool)) return;
    let reason = null;
    try {
      const body = JSON.stringify({ format: FORMAT, tool: input.tool, args: output && output.args, cwd: directory });
      reason = refusalOf(await runScript(guard, body, guardDeadline));
    } catch {}
    if (reason) throw new Error(reason);
  };

  return {
    "tool.execute.before": checkEdit,
    event: async ({ event }) => {
      try {
        observe(event);
      } catch {}
    },
    // The configuration this session was launched with is Octoboard's for this one process: a
    // shell the agent starts gets the user's own back, so an OpenCode started from it is not
    // given this plugin, this session's tools or its role. An empty value reads as unset. The
    // same goes for the user's setting that turns the terminal title off, which the launch
    // overrules for this process where their shell sets it.
    // OpenCode hands over the configuration it merged from the user's files, the inline value
    // and `OPENCODE_PERMISSION`, before it builds the agents' rules from it. Only that object is
    // changed: no file is written.
    config: async (config) => {
      try {
        writeRules(config, data);
      } catch {}
    },
    "shell.env": async (_input, output) => {
      try {
        output.env.OPENCODE_CONFIG_CONTENT = process.env.OCTOBOARD_OPENCODE_ORIGINAL_CONFIG || "";
        output.env.OCTOBOARD_OPENCODE_ORIGINAL_CONFIG = "";
        Object.assign(output.env, data.user_claude_env);
        if (process.env.OCTOBOARD_OPENCODE_SERVER) {
          output.env.OPENCODE_SERVER_PASSWORD = "";
          output.env.OPENCODE_SERVER_USERNAME = "";
          output.env.OCTOBOARD_OPENCODE_SERVER = "";
          output.env.OCTOBOARD_OPENCODE_ROUTE = "";
        }
        if (typeof data.user_title_off === "string") output.env.OPENCODE_DISABLE_TERMINAL_TITLE = data.user_title_off;
      } catch {}
    },
  };
};
