/** A type predicate for the values of a closed string union, for reading an enum the daemon sends (`Received<T>`, the
 * union or any other string): it says whether `raw` is one this client knows, and narrows it to `T` where it is.
 *
 * `known` lists every value of `T` as a key, so TypeScript checks that none is missing and none is extra: a value added
 * to the protocol's enum fails to compile here until it is listed, which is the place to decide what the client does
 * with it. A value that is not listed (a newer daemon's) is not `T`, and the reader of it shows the record neutrally
 * and acts on nothing.
 *
 * ```ts
 * const isSessionStatus = knownValues<SessionStatus>({ starting: true, working: true, idle: true });
 * const status = isSessionStatus(session.status) ? session.status : undefined;
 * ```
 *
 * `null` and `undefined` are accepted, so an optional field is read the same way; neither is known. */
export function knownValues<T extends string>(known: Record<T, true>): (raw: string | null | undefined) => raw is T {
  return (raw): raw is T => typeof raw === "string" && Object.hasOwn(known, raw);
}
