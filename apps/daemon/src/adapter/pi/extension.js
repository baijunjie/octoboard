// Octoboard's extension for Pi, loaded with `--extension` on every launch and resume of a session.
// Written by the daemon into the session's scratch directory beside `octoboard-pi.json`, which
// holds what differs per session; the daemon's Pi adapter (`mod.rs` beside this file) says what.
//
// It gives the session its Octoboard MCP server's tools, declared to the model as
// `mcp__octoboard__<tool>`: by registering the server with Pi, or, where the user's own `mcp.json`
// names a server that would take the registered one's place, by speaking to the server itself and
// registering each tool. It forwards through the session's hook script, which posts to the daemon:
// Pi's `session_start`, once more when the session it started has been written to its file, each
// prompt, and what a run said once Pi has settled after it. Where the session is kept to its own
// project, it asks the session's guard script about every call of Pi's file-writing tools and
// refuses one the guard refuses, with the guard's reason. It attaches the images Octoboard pasted
// into the session, whose paths Pi would send as text. It never throws into Pi, which exits on an
// extension that throws while loading, and never prints: whatever fails is left silent, as a
// hook's failure is.

import { spawn } from "node:child_process";
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

const DATA_FILE = "octoboard-pi.json";

// How long a prompt waits for the tools this extension registers itself, as Pi waits for the
// servers it connects (1.5.2).
const TOOLS_WAIT_MS = 10_000;

// The most of a run's last message that is forwarded, in UTF-16 units: the hook callback's body
// has a limit, which a message this long stays well under however it is escaped.
const MESSAGE_LIMIT = 200_000;

function readData() {
  try {
    const here = dirname(fileURLToPath(import.meta.url));
    return JSON.parse(readFileSync(join(here, DATA_FILE), "utf8"));
  } catch {
    return null;
  }
}

// The payload goes to the hook script on its stdin; nothing is read back. Resolves once the script
// has exited, which it does once the daemon has taken the event, or could not be reached, or
// after `waitMs`, whichever comes first; never rejects.
function forward(hook, payload, waitMs = 0) {
  return new Promise((settle) => {
    let timer;
    const done = () => {
      clearTimeout(timer);
      settle();
    };
    try {
      // A batch file runs only through the shell on Windows, quoted for a path with spaces.
      const windows = globalThis.process?.platform === "win32";
      const child = spawn(windows ? `"${hook}"` : hook, [], {
        stdio: ["pipe", "ignore", "ignore"],
        shell: windows,
        windowsHide: true,
      });
      timer = setTimeout(done, waitMs);
      child.on("error", done);
      child.on("close", done);
      child.stdin.on("error", () => {});
      child.stdin.end(JSON.stringify(payload));
      child.unref();
    } catch {
      // Silent, as a hook's failure is.
      done();
    }
  });
}

// The id of the session Pi opened, only once Pi has written its file: a session `/new` opens is
// written at its first message, and until then a resume by its id finds nothing.
function storedSessionId(ctx) {
  try {
    const file = ctx.sessionManager.getSessionFile();
    return file && existsSync(file) ? ctx.sessionManager.getSessionId() : undefined;
  } catch {
    return undefined;
  }
}

// Whether a `mcp.json` Pi may read names a server that takes the place of one registered as
// `name`: the configuration directory's, or the working directory's `.pi/mcp.json`, which Pi reads
// for a trusted project. Pi counts names that differ only in `-` and `_` as one server, and a
// server of a file wins over a registered one (1.5.2). An entry Pi itself would skip (an invalid
// one, an untrusted project's) counts here too: taking the tools' registration over where Pi
// would have served them costs nothing, and the reverse leaves the session without them.
function namedInUsersFiles(name) {
  const key = (server) => server.replace(/-/g, "_");
  const env = globalThis.process?.env?.PI_CODING_AGENT_DIR;
  const home = homedir();
  const agentDir = env ? env.replace(/^~(?=$|[\\/])/, home) : join(home, ".pi", "agent");
  for (const file of [join(agentDir, "mcp.json"), join(process.cwd(), ".pi", "mcp.json")]) {
    try {
      const servers = JSON.parse(readFileSync(file, "utf8").replace(/^﻿/, "")).mcpServers;
      if (Object.keys(servers ?? {}).some((server) => key(server) === key(name))) return true;
    } catch {
      // No such file, or one Pi cannot read either.
    }
  }
  return false;
}

// A client of an MCP server run as a child process, one JSON-RPC message a line on its stdin and
// stdout. `onToolsChanged` is called when the server says its tool list has changed.
function connect(mcp, onToolsChanged) {
  const waiting = new Map();
  let nextId = 1;
  let gone;
  const fail = (error) => {
    gone ??= error;
    for (const { reject } of waiting.values()) reject(gone);
    waiting.clear();
  };
  const child = spawn(mcp.command, mcp.args, { stdio: ["pipe", "pipe", "ignore"], windowsHide: true });
  child.on("error", (error) => fail(error));
  child.on("exit", () => fail(new Error("Octoboard's MCP server has exited.")));
  child.stdin.on("error", () => {});
  const send = (message) => {
    try {
      child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
    } catch {
      // The exit that follows fails what waits.
    }
  };
  createInterface({ input: child.stdout }).on("line", (line) => {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message?.method === undefined) {
      const call = waiting.get(message?.id);
      if (!call) return;
      waiting.delete(message.id);
      if (message.error) call.reject(new Error(message.error.message ?? "The MCP request failed."));
      else call.resolve(message.result);
    } else if (message.method === "notifications/tools/list_changed") {
      onToolsChanged();
    } else if (message.id !== undefined) {
      // A request of the server's: a ping is answered, and nothing else is offered.
      if (message.method === "ping") send({ id: message.id, result: {} });
      else send({ id: message.id, error: { code: -32601, message: "Method not found" } });
    }
  });
  // Resolves with the result; rejects where the server answers with an error or has gone, at the
  // timeout, and when `signal` aborts, telling the server in the last two that the call is off,
  // except for `initialize`, which MCP lets no client cancel.
  const request = (method, params, signal) =>
    new Promise((resolve, reject) => {
      if (gone) return reject(gone);
      if (signal?.aborted) return reject(new Error("The call was cancelled."));
      const id = nextId++;
      const timeoutMs = mcp.timeout * 1000;
      const settle = (settled, value) => {
        clearTimeout(timer);
        signal?.removeEventListener("abort", abort);
        settled(value);
      };
      const withdraw = (error) => {
        if (!waiting.delete(id)) return;
        if (method !== "initialize") {
          send({ method: "notifications/cancelled", params: { requestId: id, reason: error.message } });
        }
        settle(reject, error);
      };
      const abort = () => withdraw(new Error("The call was cancelled."));
      const timer = setTimeout(() => withdraw(new Error(`MCP request timed out after ${timeoutMs}ms`)), timeoutMs);
      signal?.addEventListener("abort", abort, { once: true });
      waiting.set(id, {
        resolve: (value) => settle(resolve, value),
        reject: (error) => settle(reject, error),
      });
      send({ id, method, params });
    });
  const ready = request("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "octoboard-pi-extension", version: "1" },
  }).then(() => send({ method: "notifications/initialized" }));
  ready.catch(() => {});
  return {
    isGone: () => gone !== undefined,
    async listTools() {
      await ready;
      const tools = [];
      let cursor;
      do {
        const page = await request("tools/list", cursor === undefined ? {} : { cursor });
        tools.push(...(page?.tools ?? []));
        cursor = page?.nextCursor;
      } while (cursor);
      return tools;
    },
    async callTool(name, args, signal) {
      await ready;
      return request("tools/call", { name, arguments: args ?? {} }, signal);
    },
    close() {
      fail(new Error("The session has ended."));
      try {
        child.stdin.end();
        child.kill();
      } catch {
        // Already gone.
      }
    },
  };
}

// An MCP tool result as Pi's: its text blocks, any other block as its JSON, and the structured
// result as JSON where there is no block.
function toolResult(result) {
  const content = (result?.content ?? []).map((block) =>
    block?.type === "text" ? { type: "text", text: String(block.text ?? "") } : { type: "text", text: JSON.stringify(block) },
  );
  if (content.length === 0) {
    const text = result?.structuredContent === undefined ? "" : JSON.stringify(result.structuredContent);
    content.push({ type: "text", text: text || (result?.isError ? "The tool reported an error." : "Done.") });
  }
  return { content, details: undefined, ...(result?.isError ? { isError: true } : {}) };
}

// Registers the server's tools as this extension's own, under the names Pi would give them, and
// answers their calls from the server. A tool of an extension wins over a tool of the same name
// that Pi registers for a server of the user's (1.5.2), whose other tools stay as they are.
function serveTools(pi, mcp) {
  let server;
  // Never reset: Pi loads the extension anew for the session that follows this one.
  let ended = false;
  // Settles once the tools of the server now open are registered, or could not be.
  let registered = Promise.resolve();
  // The definitions registered, by the server's name for the tool, to withdraw one it drops.
  const definitions = new Map();
  const register = async (from) => {
    const tools = await from.listTools();
    if (from !== server) return;
    const offered = new Set();
    for (const tool of tools) {
      if (typeof tool?.name !== "string") continue;
      offered.add(tool.name);
      const schema = tool.inputSchema && typeof tool.inputSchema === "object" ? tool.inputSchema : {};
      const definition = {
        name: `mcp__${mcp.name}__${tool.name}`.replace(/[^A-Za-z0-9_]/g, "_"),
        label: `${mcp.name}/${tool.name}`,
        description: tool.description?.trim() || `MCP tool ${tool.name} from server ${mcp.name}`,
        parameters: { ...schema, type: schema.type ?? "object", properties: schema.properties ?? {} },
        async execute(_toolCallId, params, signal) {
          // A server that has gone is started again by the next call, as Pi reconnects one.
          if (server.isGone() && !ended) open();
          return toolResult(await server.callTool(tool.name, params, signal));
        },
      };
      definitions.set(tool.name, definition);
      pi.registerTool(definition);
    }
    // Pi has no way to take a tool back: one the server no longer offers is hidden.
    for (const [name, definition] of definitions) {
      if (offered.has(name)) continue;
      definitions.delete(name);
      pi.registerTool({ ...definition, exposure: "hidden" });
    }
  };
  const open = () => {
    const opened = connect(mcp, () => register(opened).catch(() => {}));
    server = opened;
    registered = register(opened).catch(() => {});
  };
  // Not while the extension loads: Pi loads extensions for runs that start no session too.
  pi.on("session_start", () => {
    try {
      open();
    } catch {
      // The server could not be started; the session runs without the tools.
    }
  });
  // A prompt waits for the tools, so that its first request declares them, but not for ever.
  pi.on("before_agent_start", async () => {
    let timer;
    await Promise.race([registered, new Promise((resolve) => (timer = setTimeout(resolve, TOOLS_WAIT_MS)))]);
    clearTimeout(timer);
  });
  pi.on("session_shutdown", () => {
    ended = true;
    server?.close();
  });
}

// The start of a text that is longer than the limit, never ending inside a character: half of a
// surrogate pair is written as an escape the daemon refuses the whole payload for.
function clipped(text) {
  if (text.length <= MESSAGE_LIMIT) return text;
  const last = text.charCodeAt(MESSAGE_LIMIT - 1);
  return text.slice(0, last >= 0xd800 && last <= 0xdbff ? MESSAGE_LIMIT - 1 : MESSAGE_LIMIT);
}

// What the last assistant message of a run said: its text, or the error it ended on.
function lastSaid(last) {
  if (!last) return undefined;
  const text = textOf(last.content);
  const said = text.trim() ? text : last.stopReason === "error" ? last.errorMessage : undefined;
  return typeof said === "string" && said.trim() ? clipped(said) : undefined;
}

// The text of a message's content: a string, or its text blocks joined.
function textOf(content) {
  if (typeof content === "string") return content;
  return (content ?? [])
    .filter((block) => block?.type === "text" && typeof block.text === "string")
    .map((block) => block.text)
    .join("");
}

// The characters Pi reads as spaces in a tool's path (`normalizeUnicodeSpaces`, 1.5.2).
const UNICODE_SPACES = /[\u00A0\u2000-\u200A\u202F\u205F\u3000]/g;

// The file a write or edit call's `path` names, resolved as Pi resolves it before it writes
// (`resolveToCwd`, 1.5.2): unicode spaces read as spaces, a leading `@` dropped, `~` the home
// directory, and a relative path against the session's working directory.
function writtenPath(path, cwd) {
  let normalized = path.replace(UNICODE_SPACES, " ");
  if (normalized.startsWith("@")) normalized = normalized.slice(1);
  if (normalized === "~") normalized = homedir();
  else if (normalized.startsWith("~/") || (globalThis.process?.platform === "win32" && normalized.startsWith("~\\"))) {
    normalized = join(homedir(), normalized.slice(2));
  } else if (/^file:\/\//.test(normalized)) {
    normalized = fileURLToPath(normalized);
  }
  return isAbsolute(normalized) ? resolve(normalized) : resolve(cwd, normalized);
}

// The most of the guard's answer that is read.
const GUARD_OUTPUT_LIMIT = 64 * 1024;

// Asks Octoboard's cross-project guard about `payload`, through the session's guard script, and
// resolves with its refusal (`{ block: true, reason }`) or `undefined`. The guard fails open, and
// so does this: a script that cannot run, an answer that is not one, and a guard past `timeoutMs`
// all let the call go ahead.
function askGuard(script, payload, timeoutMs) {
  return new Promise((settle) => {
    let done = false;
    let timer;
    const finish = (value) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      settle(value);
    };
    try {
      const windows = globalThis.process?.platform === "win32";
      const child = spawn(windows ? `"${script}"` : script, [], {
        stdio: ["pipe", "pipe", "ignore"],
        shell: windows,
        windowsHide: true,
      });
      let out = "";
      timer = setTimeout(() => {
        try {
          child.kill();
        } catch {
          // Gone already.
        }
        finish(undefined);
      }, timeoutMs);
      child.on("error", () => finish(undefined));
      child.stdin.on("error", () => {});
      child.stdout.on("data", (chunk) => {
        if (out.length < GUARD_OUTPUT_LIMIT) out += chunk;
      });
      child.on("close", () => {
        try {
          const decision = JSON.parse(out);
          finish(decision?.block === true && typeof decision.reason === "string" ? { block: true, reason: decision.reason } : undefined);
        } catch {
          finish(undefined);
        }
      });
      child.stdin.end(JSON.stringify(payload));
    } catch {
      finish(undefined);
    }
  });
}

// `text` as a regular expression's source that matches it literally.
function literally(text) {
  return text.replace(/[\\^$.*+?()[\]{}|/-]/g, "\\$&");
}

// An image Octoboard saved for this session, as its path appears in the input: bare, or in single
// quotes as a POSIX shell quotes a path with whitespace (`'\''` for a quote inside it). Octoboard
// pastes each one on its own, so the next path or the text can follow with no space between.
// `pasted` is what the data file says of the files: their directory, the start of their names and
// their extensions; a name goes on with two runs of digits, as the daemon names it
// (`session::pasted_image`).
function pastedImagePattern(pasted) {
  const extensions = Object.keys(pasted.types).map(literally).join("|");
  const file = `[\\\\/]${literally(pasted.directory)}[\\\\/]${literally(pasted.prefix)}\\d+-\\d+\\.(${extensions})`;
  return new RegExp(`'((?:[^']|'\\\\'')*?${file})'|((?:[A-Za-z]:)?[\\\\/][^\\s']*?${file})`, "g");
}

// The real path of `path`, or `undefined` where it does not lead anywhere.
function realPath(path) {
  try {
    return realpathSync(path);
  } catch {
    return undefined;
  }
}

// `text` with the images Octoboard saved for this session taken out of it, and those images as
// Pi's attachments: Pi 1.5.2 sends a pasted path as text, which the model reads as a path, so the
// extension attaches the file itself. Only a file of this session's own `pasted` directory, beside
// the extension in its scratch directory, is attached; any other path is left as it was written.
// `pasted` is what the data file says of those files.
function pastedImages(text, pastedDir, pasted) {
  const images = [];
  const rest = text.replace(pastedImagePattern(pasted), (whole, quoted, quotedType, bare, bareType) => {
    const path = quoted === undefined ? bare : quoted.replace(/'\\''/g, "'");
    const real = realPath(path);
    if (!real || realPath(dirname(real)) !== pastedDir) return whole;
    try {
      const data = readFileSync(real).toString("base64");
      images.push({ type: "image", mimeType: pasted.types[quotedType ?? bareType], data });
      return "";
    } catch {
      return whole;
    }
  });
  return { text: rest.trim(), images };
}

function serveChannel(pi, command) {
  let child;
  let unsubscribe;
  let candidates = [];
  let escape;
  let inputSequence = 0;
  const report = (message, outcome, reason) => {
    if (child !== message.child) return;
    try {
      child.stdin.write(`${JSON.stringify({ id: message.id, outcome, reason })}\n`);
    } catch {
      // The helper is already gone.
    }
  };
  const stop = () => {
    try {
      unsubscribe?.();
    } catch {
      // Pi can have retired the extension context before the helper exits.
    }
    unsubscribe = undefined;
    escape = undefined;
    for (const candidate of candidates) clearTimeout(candidate.timer);
    candidates = [];
    const running = child;
    child = undefined;
    if (!running) return;
    try {
      running.stdin.end();
      running.kill();
    } catch {
      // The helper has exited already.
    }
  };

  pi.on("session_start", (_, ctx) => {
    if (child) return;
    try {
      // Starting the helper directly keeps its pipes intact on Windows. A detached cmd.exe
      // passed the polls through to the daemon, but no message came back on the Node pipe.
      const running = spawn(command.command, command.args, {
        stdio: ["pipe", "pipe", "ignore"], windowsHide: true, detached: true,
      });
      child = running;
      const forget = () => {
        if (child === running) stop();
      };
      running.once("error", forget);
      running.once("exit", forget);
      running.stdin.on("error", () => {});
      unsubscribe = ctx.ui?.onTerminalInput?.((data) => {
        if (data !== "\x1b") return;
        try {
          escape = { draft: ctx.ui.getEditorText(), confirmed: candidates.filter((message) => message.taken) };
        } catch {
          escape = undefined;
        }
      });
      createInterface({ input: running.stdout }).on("line", (line) => {
        try {
          const { id, text } = JSON.parse(line);
          if (typeof id !== "string" || typeof text !== "string" || child !== running) return;
          const candidate = { id, text, child: running, observed: false, taken: false, timer: undefined };
          candidate.timer = setTimeout(() => {
            candidates = candidates.filter((message) => message !== candidate || message.taken);
          }, 10_000);
          candidate.timer.unref();
          candidates.push(candidate);
          try {
            pi.sendUserMessage(text, { deliverAs: "followUp", expandPromptTemplates: false });
          } catch {
            candidates = candidates.filter((message) => message !== candidate);
            clearTimeout(candidate.timer);
            report(candidate, "refused", "Pi did not accept the message");
          }
        } catch {
          // A bad or closed channel must not throw into Pi.
        }
      });
    } catch {
      stop();
    }
  });

  pi.on("input", (event, ctx) => {
    const sequence = ++inputSequence;
    if (event.source !== "extension") return;
    const candidate = candidates.find((message) => !message.observed && message.text === event.text);
    if (!candidate) return;
    candidate.observed = true;
    // Pi's public sendUserMessage returns before the asynchronous input handlers finish.
    // A transition from no pending messages proves admission when this was the only input.
    if (event.streamingBehavior !== "followUp" || ctx.hasPendingMessages()) return;
    setImmediate(() => {
      try {
        if (sequence !== inputSequence || candidate.taken || candidates.length !== 1 || candidates[0] !== candidate) return;
        if (!ctx.hasPendingMessages()) return;
        candidate.taken = true;
        report(candidate, "taken");
      } catch {
        // A session might have closed while input was being processed.
      }
    });
  });
  pi.on("message_start", (event) => {
    if (event.message?.role !== "user") return;
    const text = textOf(event.message.content);
    const candidate = candidates.find((message) => message.observed && message.text === text);
    if (!candidate) return;
    candidates = candidates.filter((message) => message !== candidate);
    clearTimeout(candidate.timer);
    if (!candidate.taken) report(candidate, "taken");
  });
  pi.on("agent_settled", (event, ctx) => {
    const interrupted = escape;
    escape = undefined;
    if (!event.aborted || !interrupted?.confirmed.length) {
      candidates = candidates.filter((message) => !message.taken);
      return;
    }
    try {
      const returned = interrupted.confirmed.filter((message) => candidates.includes(message));
      if (!returned.length) return;
      // Pi restores the queue ahead of the draft. Only remove it when the entire editor matches
      // what the channel knows was pending and what the editor held before Escape.
      const restored = [...returned.map((message) => message.text), interrupted.draft]
        .filter((text) => text.trim())
        .join("\n\n");
      if (ctx.ui.getEditorText() !== restored) return;
      ctx.ui.setEditorText(interrupted.draft);
      if (ctx.ui.getEditorText() !== interrupted.draft) return;
      for (const message of returned) {
        clearTimeout(message.timer);
        report(message, "returned");
      }
    } catch {
      // Leave any unrecognized editor state to the user.
    } finally {
      candidates = candidates.filter((message) => !interrupted.confirmed.includes(message));
    }
  });
  pi.on("session_shutdown", stop);
}

function reportContextUsage(pi, data) {
  let previous;
  let pending = Promise.resolve();
  const report = (_, ctx) => {
    try {
      if (typeof ctx?.getContextUsage !== "function") return;
      const usage = ctx.getContextUsage();
      const tokens = usage?.tokens ?? null;
      const window = usage?.contextWindow ?? null;
      if (previous?.tokens === tokens && previous?.window === window) return;
      previous = { tokens, window };
      // Pi changes its projection before these events, even when a tree rewind writes no
      // entry. Send in that order so an older sample cannot restore the abandoned leaf.
      pending = pending.then(() =>
        forward(data.hook, { hook_event_name: "context_usage", tokens, window }, data.hook_wait_ms),
      );
    } catch {
      // Context reporting must leave the agent's event alone.
    }
  };
  for (const event of ["session_start", "session_tree", "session_compact", "model_select", "context", "turn_end", "agent_settled"]) {
    pi.on(event, report);
  }
}

export default function octoboard(pi) {
  const data = readData();
  if (!data) return;
  if (typeof data.channel?.command === "string" && Array.isArray(data.channel.args)) serveChannel(pi, data.channel);
  reportContextUsage(pi, data);
  // An image path Octoboard pasted becomes an attachment before Pi expands the text. A message
  // that names none goes on as it is. The directory is looked for at each message: Octoboard
  // makes it at the session's first pasted image.
  pi.on("input", (event) => {
    try {
      if (typeof event?.text !== "string") return undefined;
      const pastedDir = realPath(join(dirname(fileURLToPath(import.meta.url)), data.pasted.directory));
      if (!pastedDir) return undefined;
      const found = pastedImages(event.text, pastedDir, data.pasted);
      if (found.images.length === 0) return undefined;
      return { action: "transform", text: found.text, images: [...(event.images ?? []), ...found.images] };
    } catch {
      // No image is attached then; the paths go to the model as text.
      return undefined;
    }
  });
  // The prompt of the run Pi is starting has been forwarded, and its own `message_start`, the
  // run's first of a user message, is not to be forwarded again. Each run sets it afresh, and a
  // run that ends without that `message_start` (cancelled while it was starting) clears it at its
  // end, so a later run's steering message is not taken for it.
  let openingForwarded = false;
  // A run's prompt is forwarded before the run starts: Pi awaits this handler before it reports
  // `working` (1.5.2), so the prompt reaches the daemon before the report, as long as the hook
  // script exits within `hook_wait_ms`. Forwarded at `message_start` instead, it could arrive after
  // a turn that ended at once (a refused request, an instant Esc) was closed, and open a turn of
  // its own. A run of an extension's custom message has no prompt to forward.
  pi.on("before_agent_start", async (event) => {
    try {
      openingForwarded = typeof event?.prompt === "string";
      if (!openingForwarded) return;
      await forward(data.hook, { hook_event_name: "message_start", prompt: clipped(event.prompt) }, data.hook_wait_ms);
    } catch {
      // Silent, as a hook's failure is.
    }
  });
  // Every other prompt that enters the conversation, a steering message Pi takes into a run, as
  // the model is given it: a message Esc puts back in the input box never gets here.
  pi.on("message_start", (event) => {
    try {
      if (event?.message?.role !== "user") return;
      if (openingForwarded) {
        openingForwarded = false;
        return;
      }
      forward(data.hook, { hook_event_name: "message_start", prompt: clipped(textOf(event.message.content)) });
    } catch {
      // Silent, as a hook's failure is.
    }
  });
  if (typeof data.guard === "string") {
    pi.on("tool_call", async (event, ctx) => {
      try {
        const path = event?.input?.path;
        // Pi's tools that write a file, each naming it in `path`. Its shell tool can write too, as
        // the other agents' can; their guards leave it alone as well.
        if (!data.file_writing_tools.includes(event?.toolName) || typeof path !== "string") return undefined;
        const cwd = ctx?.cwd ?? process.cwd();
        return await askGuard(
          data.guard,
          { tool_name: event.toolName, tool_input: { path: writtenPath(path, cwd) }, cwd },
          data.guard_timeout_ms,
        );
      } catch {
        return undefined;
      }
    });
  }
  try {
    if (namedInUsersFiles(data.mcp.name)) {
      serveTools(pi, data.mcp);
    } else {
      pi.registerMcpServer(data.mcp.name, {
        command: data.mcp.command,
        args: data.mcp.args,
        exposure: "direct",
        timeout: data.mcp.timeout,
      });
    }
  } catch {
    // Pi refuses an invalid registration by throwing; the session runs without the tools then.
  }
  // The id last given to the daemon, which records it as the conversation a resume opens.
  let named;
  // What the latest run said, kept until Pi settles: it may run again by itself, after an error
  // it retries or for a message that was waiting.
  let said;
  let failed = false;
  let running = false;
  let promptDepth = 0;
  let statusTail = Promise.resolve();
  const reportStatus = (payload) => {
    statusTail = statusTail.then(() => forward(data.hook, payload, data.hook_wait_ms));
    return statusTail;
  };
  if (data.report_turn_status) {
    pi.on("agent_start", () => {
      if (running) return;
      running = true;
      return reportStatus({ hook_event_name: "agent_status", turn_status: "working" });
    });
    pi.on("ui_prompt_start", () => {
      if (!running || ++promptDepth !== 1) return;
      return reportStatus({ hook_event_name: "agent_status", turn_status: "blocked" });
    });
    pi.on("ui_prompt_end", () => {
      if (!running || promptDepth === 0 || --promptDepth !== 0) return;
      return reportStatus({ hook_event_name: "agent_status", turn_status: "working" });
    });
  }
  pi.on("session_start", (event, ctx) => {
    named = storedSessionId(ctx);
    forward(data.hook, { hook_event_name: "session_start", session_id: named, reason: event?.reason });
  });
  // A session `/new` or `/fork` opens is written at its first message, after its `session_start`,
  // so its id is given once a run has ended with its file there: the same event again, which the
  // daemon reads as it reads the one after `/resume`.
  pi.on("agent_end", (event, ctx) => {
    openingForwarded = false;
    try {
      const last = (event?.messages ?? []).findLast((message) => message?.role === "assistant");
      failed = last?.stopReason === "error";
      said = lastSaid(last);
    } catch {
      said = undefined;
    }
    const id = storedSessionId(ctx);
    if (!id || id === named) return;
    named = id;
    forward(data.hook, { hook_event_name: "session_start", session_id: id, reason: "saved" });
  });
  // Pi reports `done` or `error` as it settles, with the session's name or the error's first line
  // as the report's message, never what the model said: the daemon takes that from here. A run
  // the user cancelled is reported `idle`, which ends its turn with nothing to say: what it said
  // is dropped, or it would go out for a later settling that follows no run, a manual `/compact`.
  pi.on("agent_settled", (event) => {
    openingForwarded = false;
    const report = running;
    running = false;
    promptDepth = 0;
    const payload = { hook_event_name: "agent_settled", last_assistant_message: said ?? null };
    const turnStatus = failed ? "error" : "done";
    said = undefined;
    failed = false;
    if (event?.aborted) {
      if (report) return reportStatus({ hook_event_name: "agent_status", turn_status: "idle" });
      return;
    }
    if (report) return reportStatus({ ...payload, turn_status: turnStatus });
    forward(data.hook, payload);
  });
}
