import { useEffect, useRef, useState } from "react";

import type { OrphanedMemory } from "../protocol";
import { useDaemon, useDaemonStore } from "../store";

/** The places whose memory no project is at any more (what removing their projects left behind), kept current for as
 * long as the caller is mounted. Asked for at mount and again after every `snapshot`, since the daemon sends it in
 * neither, and replaced by each broadcast of the set. A broadcast that arrives while a listing is out is newer than that
 * reply, which is then dropped. `failed` says the first listing could not be had (`failure`; `retry` asks again,
 * `retrying` while it is out); a failure after one succeeded leaves what is held. */
export function useOrphanedMemory(): {
  places: readonly OrphanedMemory[];
  status: "loading" | "ready" | "failed";
  failure: unknown;
  retry: () => void;
  retrying: boolean;
} {
  const { request, onMemoryChange } = useDaemon();
  const snapshotEpoch = useDaemonStore((s) => s.snapshotEpoch);
  const [places, setPlaces] = useState<readonly OrphanedMemory[]>([]);
  const [status, setStatus] = useState<"loading" | "ready" | "failed">("loading");
  const [failure, setFailure] = useState<unknown>(undefined);
  const [attempt, setAttempt] = useState(0);
  const [retrying, setRetrying] = useState(false);
  const broadcast = useRef(0);

  useEffect(
    () =>
      onMemoryChange((event) => {
        if (event.type !== "orphaned_memory_list") return;
        broadcast.current += 1;
        setPlaces(event.places);
        setFailure(undefined);
        setStatus("ready");
      }),
    [],
  );

  useEffect(() => {
    let current = true;
    const seen = broadcast.current;
    request({ type: "list_orphaned_memories" }).then(
      (reply) => {
        if (!current) return;
        setRetrying(false);
        if (broadcast.current !== seen) return;
        setPlaces(reply.places);
        setFailure(undefined);
        setStatus("ready");
      },
      (error: unknown) => {
        if (!current) return;
        setRetrying(false);
        setFailure(error);
        setStatus((now) => (now === "ready" ? "ready" : "failed"));
      },
    );
    return () => {
      current = false;
    };
  }, [snapshotEpoch, attempt]);

  return {
    places,
    status,
    failure,
    retry: () => {
      setRetrying(true);
      setAttempt((n) => n + 1);
    },
    retrying,
  };
}
