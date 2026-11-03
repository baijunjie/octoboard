import type { Event } from "../protocol";

/** A message the daemon words by its `code` and `params` (see "Coded messages" in `apps/daemon/PROTOCOL.md`), with its
 * own English `message` for a code the client does not know: an `error` reply's, and every failure the daemon reports
 * the same way (a quota reading, a models list, a login). */
export type CodedMessage = Pick<Extract<Event, { type: "error" }>, "code" | "params" | "message">;

/** The named values a daemon message is filled with. `console`, `project` and `session` are record ids, which
 * `daemonMessage` shows as the record's name; the rest is text to show as is. */
export type MessageParams = CodedMessage["params"];

/** A failure with a code and its params (see "Coded messages" in `apps/daemon/PROTOCOL.md`), for a caller that branches
 * on the failure, and a `message` to show. The daemon's `error` replies arrive as one: `daemon-client.ts` puts the
 * daemon's English text in `message`, and the store replaces it with the wording in the current language. The client
 * raises its own (`connection_closed`, `request_timed_out`, `unexpected_reply`), and the UI words a failure it detects
 * itself as one (`ActionableError`, and `errorPresentation` for anything else thrown). */
export class CodedError extends Error {
  readonly code: string;
  readonly params: MessageParams;
  constructor(message: string, code: string, params: MessageParams) {
    super(message);
    this.code = code;
    this.params = params;
  }
}
