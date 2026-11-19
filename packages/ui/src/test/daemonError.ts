import { CodedError, type MessageParams } from "../errors/codedError";
import { daemonMessage, NO_RECORDS, type RecordNames } from "../errors/daemonMessage";

/** The daemon's refusal `code` with `params`, as a faked request rejects with it: a `CodedError` whose message is the
 * catalog's sentence for the code and params, in English, as the store words it before a caller sees it. `records`
 * names what a record id param stands for; none by default, so an id shows as itself. */
export function daemonError(code: string, params: MessageParams, records: RecordNames = NO_RECORDS): CodedError {
  return new CodedError(daemonMessage("en", code, params, "", records), code, params);
}
