// What the fixture daemon answers for the composer queue: putting a message on a session's queue, editing, re-moding,
// moving, sending and withdrawing one, and a session's default send mode, each change broadcast as the whole queue
// (`session_message_queue`) as the daemon does.
import type { QueuedMessage } from "../protocol";
import { applyDaemonEvent } from "../store";
import type { FixtureContext, Handlers } from "./fixtureContext";

/** The replies about the composer queue. Where it differs from the daemon: a message sent (now, or a waiting one sent
 * by hand) leaves the queue at once, as if delivery wrote it straight away, and a message set to go once the agent is
 * idle is never sent by itself, so a scenario's queue stays as it shows it. A scenario's `composerRefusal` refuses every
 * request that puts or sends a message, for a refusal shown in place. */
export function createComposerReplies({ scenario, store, fail }: FixtureContext): Handlers {
  let created = 0;
  const queueOf = (session: string): QueuedMessage[] => [...(store.getState().messageQueues?.get(session) ?? [])];
  const broadcast = (session: string, messages: QueuedMessage[]) => applyDaemonEvent(store, { type: "session_message_queue", session, messages });
  const sessionOf = (id: string) => {
    const session = store.getState().sessions.get(id);
    if (!session) return fail({ code: "unknown_session", params: { session: id }, message: `unknown session ${id}` });
    if (session.status === "archived") return fail({ code: "session_archived", params: { session: id }, message: "the session is archived" });
    return session;
  };
  const messageOf = (session: string, id: string) => {
    const message = queueOf(session).find((queued) => queued.id === id);
    if (!message) return fail({ code: "queued_message_not_found", params: { session, message: id }, message: `no queued message ${id}` });
    return message;
  };
  const refuse = () => {
    if (scenario.composerRefusal) fail(scenario.composerRefusal);
  };
  const replace = (session: string, id: string, change: (message: QueuedMessage) => QueuedMessage | undefined) => {
    broadcast(session, queueOf(session).flatMap((message) => {
      if (message.id !== id) return [message];
      const changed = change(message);
      return changed ? [changed] : [];
    }));
  };
  return {
    create_queued_message: (body) => {
      sessionOf(body.session);
      refuse();
      if (body.text.trim() === "" && (body.images ?? []).length === 0 && (body.files ?? []).length === 0) {
        return fail({ code: "field_required", params: { field: "text" }, message: "a message needs text, an image or a file" });
      }
      created += 1;
      const id = `queued-${created}`;
      // With no mode the session's default applies, as the daemon applies it: `now` sends at once like an explicit one.
      const mode = body.mode ?? store.getState().sessions.get(body.session)?.send_mode ?? "now";
      if (mode !== "now") {
        broadcast(body.session, [...queueOf(body.session), {
          id,
          text: body.text,
          images: body.images ?? [],
          files: body.files ?? [],
          command: body.command ?? false,
          mode: mode === "manual" ? "manual" : "when_idle",
          state: "waiting",
          created_at: Date.now(),
        }]);
      }
      return { type: "queued_message_created", id: null, message: id };
    },
    update_queued_message: (body) => {
      const message = messageOf(body.session, body.message);
      if (message.state === "sending") return fail({ code: "queued_message_sending", params: { session: body.session, message: body.message }, message: "the message is being sent" });
      replace(body.session, body.message, (current) => {
        const next: QueuedMessage = {
          ...current,
          text: body.text ?? current.text,
          images: body.images ?? current.images,
          files: body.files ?? current.files,
          command: body.command ?? current.command,
        };
        delete next.failure;
        return next;
      });
      return { type: "ack", id: null };
    },
    set_queued_message_mode: (body) => {
      const message = messageOf(body.session, body.message);
      if (message.state === "sending") return fail({ code: "queued_message_sending", params: { session: body.session, message: body.message }, message: "the message is being sent" });
      replace(body.session, body.message, (current) => ({ ...current, mode: body.mode }));
      return { type: "ack", id: null };
    },
    move_queued_message: (body) => {
      const message = messageOf(body.session, body.message);
      const others = queueOf(body.session).filter((queued) => queued.id !== body.message);
      others.splice(Math.min(body.position, others.length), 0, message);
      broadcast(body.session, others);
      return { type: "ack", id: null };
    },
    send_queued_message: (body) => {
      const message = messageOf(body.session, body.message);
      if (message.state === "sending") return fail({ code: "queued_message_sending", params: { session: body.session, message: body.message }, message: "the message is being sent" });
      refuse();
      replace(body.session, body.message, () => undefined);
      return { type: "ack", id: null };
    },
    withdraw_queued_message: (body) => {
      messageOf(body.session, body.message);
      replace(body.session, body.message, () => undefined);
      return { type: "ack", id: null };
    },
    set_session_send_mode: (body) => {
      const session = store.getState().sessions.get(body.session);
      if (!session) return fail({ code: "unknown_session", params: { session: body.session }, message: `unknown session ${body.session}` });
      applyDaemonEvent(store, { type: "session_upserted", session: { ...session, send_mode: body.mode } });
      return { type: "ack", id: null };
    },
  };
}
