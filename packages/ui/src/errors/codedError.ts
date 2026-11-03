import type { MessageParams } from "../protocol";

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
