import { useEffect, useRef, useState } from "react";

import { useLatest } from "../../components/hooks/useLatest";
import type { MemoryEntry } from "../../protocol";
import { changeScope, useDaemon, useDaemonStore } from "../../store";
import { scopeMatches, targetKey, targetRequest, type MemoryTarget } from "./memoryTarget";
import { EMPTY_MEMORY, staleContents, withChange, withListing, withRead, type HeldMemory, type EntryChangeEvent } from "./memoryState";

/** A console's or a project place's memory as the daemon holds it, kept current for as long as the caller is mounted.
 *
 * The daemon sends no memory in its `snapshot`, so the index is asked for at mount and again after every `snapshot` (a
 * change made while the connection was down or too far behind is then not lost), and kept by the changes the daemon
 * broadcasts, whoever made them. A change broadcast while the index is being asked for is put over the reply, as the
 * reply may have been made before it. An entry's content is read when the caller needs it (`read`), and a content held
 * of an entry a listing finds changed is read again.
 *
 * `status` is `loading` until the first listing arrives and `failed` when it could not be had (`failure`; `retry` asks
 * again, `retrying` while it is out); a listing that fails after one succeeded leaves what is held, as the `snapshot`
 * that follows the connection coming back lists again. With `enabled` false nothing is asked for or followed: a
 * hook never enabled holds an empty memory and stays `loading`, and one disabled after it listed keeps what it held, no
 * longer kept current. */
export function useMemory(target: MemoryTarget, { enabled = true }: { enabled?: boolean } = {}): {
  memory: HeldMemory;
  status: "loading" | "ready" | "failed";
  failure: unknown;
  retry: () => void;
  retrying: boolean;
  /** How many times the listing has been asked for again, so a failure that repeats in the same words is announced again. */
  attempt: number;
  /** The content of the entry `id`: what is held when it is of the version the index has, otherwise (or with `fresh`,
   * when what the window holds may itself be behind, as after a refusal for a changed entry) read from the daemon;
   * rejects with the reason it could not be read. The entry shown is then `memory.contents.get(id)`, which is nothing
   * once it has been deleted. */
  read: (id: string, fresh?: boolean) => Promise<MemoryEntry>;
} {
  const { request, onMemoryChange } = useDaemon();
  const key = targetKey(target);
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [memory, setMemory] = useState(EMPTY_MEMORY);
  const [status, setStatus] = useState<"loading" | "ready" | "failed">("loading");
  const [failure, setFailure] = useState<unknown>(undefined);
  const [attempt, setAttempt] = useState(0);
  const [retrying, setRetrying] = useState(false);
  const held = useLatest(memory);
  // The changes broadcast since the listing now out was asked for, to put over its reply.
  const during = useRef<EntryChangeEvent[]>([]);

  const readEntry = async (id: string): Promise<MemoryEntry> => {
    const reply = await request({ type: "read_memory", ...targetRequest(target), memory: id });
    setMemory((current) => withRead(current, reply.entry));
    return reply.entry;
  };

  useEffect(
    () =>
      enabled
        ? onMemoryChange((event) => {
          const scope = changeScope(event);
          if (event.type === "orphaned_memory_list" || !scope || !scopeMatches(target, scope)) return;
          during.current.push(event);
          setMemory((current) => withChange(current, event));
        })
        : undefined,
    [key, enabled],
  );

  useEffect(() => {
    if (!enabled) return;
    let current = true;
    during.current = [];
    request({ type: "list_memories", ...targetRequest(target) }).then(
      (reply) => {
        if (!current) return;
        const before = held.current;
        const next = withListing(before, reply.memories, during.current);
        during.current = [];
        setMemory(next);
        setFailure(undefined);
        setRetrying(false);
        setStatus("ready");
        for (const id of staleContents(next)) void readEntry(id).catch(() => {});
      },
      (error: unknown) => {
        if (!current) return;
        setFailure(error);
        setRetrying(false);
        setStatus((now) => (now === "ready" ? "ready" : "failed"));
      },
    );
    return () => {
      current = false;
    };
  }, [key, snapshotEpoch, attempt, enabled]);

  const read = async (id: string, fresh = false): Promise<MemoryEntry> => {
    const content = held.current.contents.get(id);
    const line = held.current.index.find((candidate) => candidate.id === id);
    return !fresh && content && line && content.version === line.version ? content : readEntry(id);
  };

  return {
    memory,
    status,
    failure,
    retry: () => {
      setRetrying(true);
      setAttempt((n) => n + 1);
    },
    retrying,
    attempt,
    read,
  };
}
