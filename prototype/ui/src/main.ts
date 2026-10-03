import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

import { resolveDaemonPort, daemonWsUrl } from "./daemon";
import { ControlSocket } from "./control-socket";
import { TermSocket, type TermSocketStatus } from "./term-socket";
import type { Agent, ControlToClient, Role } from "./protocol";

interface SessionInfo {
  session: string;
  agent: Agent;
  pid: number;
  status: string;
}

const port = resolveDaemonPort();

const appEl = document.querySelector<HTMLDivElement>("#app")!;
const controlsEl = document.querySelector<HTMLElement>("#controls")!;
const logEl = document.querySelector<HTMLDivElement>("#log")!;
const termEl = document.querySelector<HTMLDivElement>("#term")!;

if (!port) {
  appEl.innerHTML =
    "<p style='padding:16px'>No daemon port known. Pass <code>?port=NNNN</code> in the URL " +
    "(the port obd-proto printed, or the contents of <code>$TMPDIR/obd-proto.port</code>), " +
    "or set <code>VITE_DAEMON_PORT</code> for <code>vite dev</code>.</p>";
  throw new Error("daemon port unresolved");
}

const sessions = new Map<string, SessionInfo>();
let activeSession: string | undefined;
let termSocket: TermSocket | undefined;

// `convertEol` is for a text source that already assumes `\n` means "newline, go to column 0";
// a PTY's output already carries its own `\r\n` where that is wanted, so forcing it on here
// would inject carriage returns that were never in the byte stream and corrupt exactly the
// rendering T1 checks for.
const term = new Terminal({ scrollback: 5000 });
const fitAddon = new FitAddon();
term.loadAddon(fitAddon);
term.open(termEl);
fitAddon.fit();

const control = new ControlSocket(daemonWsUrl(port, "/ws/control"));

// --- control strip -----------------------------------------------------------------------

controlsEl.innerHTML = `
  <select id="agent-select">
    <option value="claude">claude</option>
    <option value="grok">grok</option>
    <option value="codex">codex</option>
  </select>
  <select id="role-select">
    <option value="worker">worker</option>
    <option value="hub">hub</option>
  </select>
  <input id="cwd-input" type="text" placeholder="cwd (absolute path)" size="30" />
  <button id="start-btn">Start session</button>
  <span id="session-chips"></span>
  <button id="kill-btn" disabled>Kill</button>
  <button id="term-reconnect-btn">Reconnect terminal</button>
  <button id="term-disconnect-btn">Disconnect terminal</button>
  <span id="term-status" class="term-status"></span>
  <input id="message-input" type="text" placeholder="send_message text" size="30" />
  <button id="send-btn">Send</button>
`;

const agentSelect = controlsEl.querySelector<HTMLSelectElement>("#agent-select")!;
const roleSelect = controlsEl.querySelector<HTMLSelectElement>("#role-select")!;
const cwdInput = controlsEl.querySelector<HTMLInputElement>("#cwd-input")!;
const startBtn = controlsEl.querySelector<HTMLButtonElement>("#start-btn")!;
const sessionChipsEl = controlsEl.querySelector<HTMLSpanElement>("#session-chips")!;
const killBtn = controlsEl.querySelector<HTMLButtonElement>("#kill-btn")!;
const termReconnectBtn = controlsEl.querySelector<HTMLButtonElement>("#term-reconnect-btn")!;
const termDisconnectBtn = controlsEl.querySelector<HTMLButtonElement>("#term-disconnect-btn")!;
const termStatusEl = controlsEl.querySelector<HTMLSpanElement>("#term-status")!;
const messageInput = controlsEl.querySelector<HTMLInputElement>("#message-input")!;
const sendBtn = controlsEl.querySelector<HTMLButtonElement>("#send-btn")!;

startBtn.addEventListener("click", () => {
  const cwd = cwdInput.value.trim();
  if (!cwd) return;
  control.send({ type: "start_session", agent: agentSelect.value as Agent, cwd, role: roleSelect.value as Role });
});

killBtn.addEventListener("click", () => {
  if (activeSession) control.send({ type: "kill_session", session: activeSession });
});

sendBtn.addEventListener("click", () => {
  const text = messageInput.value;
  if (activeSession && text) {
    control.send({ type: "send_message", session: activeSession, text });
    messageInput.value = "";
  }
});

termReconnectBtn.addEventListener("click", () => {
  termSocket?.reconnect();
  term.focus();
});
termDisconnectBtn.addEventListener("click", () => {
  termSocket?.disconnect();
  term.focus();
});

// WebSocket.send() throws before the socket is OPEN, so the initial listing waits for "open".
control.onOpen(() => control.send({ type: "list_sessions" }));

// --- session switching ---------------------------------------------------------------------

function renderSessionChips(): void {
  sessionChipsEl.innerHTML = "";
  for (const info of sessions.values()) {
    const chip = document.createElement("span");
    chip.className = "session-chip" + (info.session === activeSession ? " active" : "");
    chip.textContent = `${info.agent}:${info.session.slice(0, 8)} [${info.status}]`;
    chip.addEventListener("click", () => switchToSession(info.session));
    sessionChipsEl.appendChild(chip);
  }
  killBtn.disabled = !activeSession;
}

function switchToSession(session: string): void {
  activeSession = session;
  renderSessionChips();
  term.reset(); // fresh screen: the replay about to arrive is this session's, not the old one's
  connectTerm(session);
  // Clicking a chip (a <span>, not part of xterm's own DOM) moves focus off xterm's hidden
  // textarea; restore it so keystrokes keep reaching the newly-attached session instead of only
  // its output being visible.
  term.focus();
}

function connectTerm(session: string): void {
  termSocket?.dispose();
  const url = daemonWsUrl(port!, `/ws/term/${session}`);
  termSocket = new TermSocket(
    url,
    {
      onData: (data) => term.write(new Uint8Array(data)),
      onStatusChange: (status) => updateTermStatus(status),
    },
    { cols: term.cols, rows: term.rows },
  );
}

function updateTermStatus(status: TermSocketStatus): void {
  termStatusEl.textContent = status;
  termStatusEl.className = `term-status ${status}`;
}

term.onData((data) => {
  if (termSocket) termSocket.sendInput(new TextEncoder().encode(data));
});

// Mouse reports (and other non-UTF-8 input) arrive through `onBinary`, not `onData`: in the
// default mouse protocol a coordinate is encoded as `32 + n`, so once the mouse is past column
// 95 the encoded byte exceeds 127 and is not valid UTF-8 — `onData` would never see it at all.
// xterm.js hands it over as a JS string of raw code units, one per byte, hence the mask back to
// a byte instead of a UTF-8 encode.
term.onBinary((data) => {
  if (termSocket) termSocket.sendInput(Uint8Array.from(data, (c) => c.charCodeAt(0) & 0xff));
});

window.addEventListener("resize", () => {
  fitAddon.fit();
  termSocket?.sendResize(term.cols, term.rows);
});

// --- control event log ----------------------------------------------------------------------

function appendLog(kind: string, text: string): void {
  const entry = document.createElement("div");
  entry.className = `log-entry ${kind}`;
  entry.textContent = text;
  logEl.appendChild(entry);
  logEl.scrollTop = logEl.scrollHeight;
}

control.onEvent((event: ControlToClient) => {
  switch (event.type) {
    case "session_started": {
      // Unconditional `set`, not "only if known": on a page reload the UI's only way to learn
      // about sessions that were already running is the `list_sessions` reply, and this is the
      // event that carries enough fields (agent, pid) to create a chip for one it has never seen
      // before. This also reattaches the terminal for the first such session below.
      sessions.set(event.session, { session: event.session, agent: event.agent, pid: event.pid, status: "working" });
      // `agent_session_id` is appended only when present so the common case (not yet known, or a
      // re-announce that didn't change it) doesn't clutter the log with "agent_session_id=undefined".
      const agentSessionIdSuffix = event.agent_session_id ? ` agent_session_id=${event.agent_session_id}` : "";
      appendLog("status", `session_started ${event.session} agent=${event.agent} pid=${event.pid}${agentSessionIdSuffix}`);
      renderSessionChips();
      if (!activeSession) switchToSession(event.session);
      break;
    }
    case "status": {
      const info = sessions.get(event.session);
      if (info) info.status = event.state;
      appendLog("status", `status ${event.session.slice(0, 8)} ${event.state} (${event.source})`);
      renderSessionChips();
      break;
    }
    case "tool_call":
      appendLog("tool_call", `tool_call ${event.session.slice(0, 8)} ${event.tool} ${JSON.stringify(event.args)}`);
      break;
    case "message_queued":
      appendLog("message_queued", `message_queued ${event.session.slice(0, 8)} ${event.reason}`);
      break;
    case "error":
      appendLog("error", `error ${event.message}`);
      break;
  }
});
