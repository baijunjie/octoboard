/** An open menu may take a window command that one of its items advertises. */
const handlers = new Set<(event: KeyboardEvent) => boolean>();

export function registerMenuCommand(handler: (event: KeyboardEvent) => boolean): () => void {
  handlers.add(handler);
  return () => { handlers.delete(handler); };
}

/** Called only after a window shortcut has matched and consumed the key, while a modal is open. */
export function runMenuCommand(event: KeyboardEvent): boolean {
  for (const handler of handlers) if (handler(event)) return true;
  return false;
}
