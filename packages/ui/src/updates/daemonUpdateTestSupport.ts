import type { ClientRequestBody, Event } from "../protocol";

/** When the install the stand-in daemon says it started began. */
const STARTED_AT = 1;

/** A daemon's answers to the requests of its update, as it gives them: a check that started, an install that started at
 * `STARTED_AT`, and an ack for any other request. For a test that mounts the update store over a stand-in daemon and
 * presses what asks. */
export async function answerDaemonUpdateRequest(body: ClientRequestBody): Promise<Event> {
  if (body.type === "check_daemon_update") return { type: "daemon_update_check", id: null, started: true, last_check_at: null };
  if (body.type === "update_daemon") return { type: "daemon_update_started", id: null, started_at: STARTED_AT };
  return { type: "ack", id: null };
}
