/** The `{ kind, port?, detail? }` a shell command rejects with, when it did: each field only where it has the type
 * the shell gives it, so a caller may read it without checking what it was handed. */
export function shellFailure(err: unknown): { kind?: string; port?: number; detail?: string } {
  if (typeof err !== "object" || err === null) return {};
  const { kind, port, detail } = err as { kind?: unknown; port?: unknown; detail?: unknown };
  return {
    kind: typeof kind === "string" ? kind : undefined,
    port: typeof port === "number" ? port : undefined,
    detail: typeof detail === "string" ? detail : undefined,
  };
}
