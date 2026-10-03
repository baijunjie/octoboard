// Wire types for prototype/PROTOCOL.md. Kept as a hand-written mirror of the daemon's
// `protocol.rs` enums rather than generated, because the daemon is frozen for this milestone —
// see prototype/README.md.

export type Agent = "claude" | "grok" | "codex";
export type Role = "hub" | "worker";
export type SessionState = "working" | "waiting_user" | "idle" | "exited";
export type StatusSource = "hook" | "process";

export type ClientToControl =
  | { type: "start_session"; agent: Agent; cwd: string; role: Role; task?: string }
  | { type: "send_message"; session: string; text: string }
  | { type: "kill_session"; session: string }
  | { type: "list_sessions" };

export type ControlToClient =
  | { type: "session_started"; session: string; agent: Agent; pid: number; agent_session_id?: string }
  | { type: "status"; session: string; state: SessionState; source: StatusSource; raw: unknown }
  | { type: "tool_call"; session: string; tool: string; args: unknown }
  | { type: "message_queued"; session: string; reason: string }
  | { type: "error"; message: string };

/** Client-sent control frame on `/ws/term/:session` (text frames only). */
export type TermControl = { type: "resize"; cols: number; rows: number };
