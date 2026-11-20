// @vitest-environment jsdom
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { afterQueuedWork } from "../test/clock";
import { mountHost } from "../test/mountHost";
import { createDropUploads, dropUploads, useDropProgress, useDropUploadsShown, type DropProgress, type DropTurn } from "./dropUploads";

(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function deferred<T = void>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

/** A task that accounts for each of `names` as a drop does (`begin`, then `end`), pausing before each ends until the test lets it. */
function steppedTask(names: string[]) {
  const gates = names.map(() => deferred());
  const turns: DropTurn[] = [];
  const task = async (turn: DropTurn) => {
    turns.push(turn);
    for (const [at, name] of names.entries()) {
      turn.begin(name);
      await gates[at].promise;
      turn.end();
    }
    return names.length;
  };
  return { task, turns, finish: (at: number) => gates[at].resolve() };
}


describe("the progress of a session's uploads", () => {
  it("is none while nothing is queued for it", () => {
    const uploads = createDropUploads();
    expect(uploads.progress("s-1")).toBeUndefined();
  });

  it("is the first file's place, the number of files queued and the first file's name as soon as a drop is queued", () => {
    const uploads = createDropUploads();
    const { task } = steppedTask(["a.txt", "b.txt"]);
    void uploads.run("s-1", ["a.txt", "b.txt"], task);
    expect(uploads.progress("s-1")).toEqual({ index: 1, total: 2, name: "a.txt" });
  });

  it("moves to the next file as each ends and names the one now going", async () => {
    const uploads = createDropUploads();
    const { task, finish } = steppedTask(["a.txt", "b.txt", "c.txt"]);
    const done = uploads.run("s-1", ["a.txt", "b.txt", "c.txt"], task);
    finish(0);
    await afterQueuedWork();
    expect(uploads.progress("s-1")).toEqual({ index: 2, total: 3, name: "b.txt" });
    finish(1);
    await afterQueuedWork();
    expect(uploads.progress("s-1")).toEqual({ index: 3, total: 3, name: "c.txt" });
    finish(2);
    await expect(done).resolves.toBe(3);
    expect(uploads.progress("s-1")).toBeUndefined();
  });

  it("is the same object until it changes", () => {
    const uploads = createDropUploads();
    const { task } = steppedTask(["a.txt"]);
    void uploads.run("s-1", ["a.txt"], task);
    expect(uploads.progress("s-1")).toBe(uploads.progress("s-1"));
  });

  it("counts every file queued for the session across drops", async () => {
    const uploads = createDropUploads();
    const first = steppedTask(["a1", "a2"]);
    const second = steppedTask(["b1", "b2", "b3"]);
    void uploads.run("s-1", ["a1", "a2"], first.task);
    void uploads.run("s-1", ["b1", "b2", "b3"], second.task);
    expect(uploads.progress("s-1")).toEqual({ index: 1, total: 5, name: "a1" });
    first.finish(0);
    await afterQueuedWork();
    expect(uploads.progress("s-1")).toEqual({ index: 2, total: 5, name: "a2" });
    first.finish(1);
    await afterQueuedWork();
    expect(uploads.progress("s-1")).toEqual({ index: 3, total: 5, name: "b1" });
  });

  it("is kept for each session apart", async () => {
    const uploads = createDropUploads();
    const first = steppedTask(["a1"]);
    const second = steppedTask(["b1", "b2"]);
    void uploads.run("s-1", ["a1"], first.task);
    void uploads.run("s-2", ["b1", "b2"], second.task);
    expect(uploads.progress("s-1")).toEqual({ index: 1, total: 1, name: "a1" });
    expect(uploads.progress("s-2")).toEqual({ index: 1, total: 2, name: "b1" });
    first.finish(0);
    await afterQueuedWork();
    expect(uploads.progress("s-1")).toBeUndefined();
    expect(uploads.progress("s-2")?.total).toBe(2);
  });

  it("counts the files a task did not reach off the total when it ends", async () => {
    const uploads = createDropUploads();
    const gate = deferred();
    const done = uploads.run("s-1", ["a", "b", "c"], async (turn) => {
      turn.begin("a");
      await gate.promise;
      turn.end();
    });
    expect(uploads.progress("s-1")?.total).toBe(3);
    gate.resolve();
    await done;
    expect(uploads.progress("s-1")).toBeUndefined();
  });

  it("leaves nothing counted for files a task never accounted for", async () => {
    const uploads = createDropUploads();
    await uploads.run("s-1", ["a", "b"], async () => "early");
    expect(uploads.progress("s-1")).toBeUndefined();
  });

  it("leaves nothing counted for a task that fails", async () => {
    const uploads = createDropUploads();
    await expect(uploads.run("s-1", ["a"], async () => {
      throw new Error("boom");
    })).rejects.toThrow("boom");
    expect(uploads.progress("s-1")).toBeUndefined();
  });
});

describe("the fraction of the file going", () => {
  it("is not told before a file starts or while it is not sent in parts", () => {
    const uploads = createDropUploads();
    const { task } = steppedTask(["a.bin"]);
    void uploads.run("s-1", ["a.bin"], task);
    expect(uploads.progress("s-1")?.fraction).toBeUndefined();
  });

  it("is the bytes acknowledged over the file's size, as the task says them", async () => {
    const uploads = createDropUploads();
    const gate = deferred();
    const done = uploads.run("s-1", ["a.bin"], async (turn) => {
      turn.begin("a.bin");
      turn.advance(0, 10);
      await gate.promise;
      turn.advance(4, 10);
      await gate.promise;
      turn.advance(10, 10);
      turn.end();
    });
    expect(uploads.progress("s-1")?.fraction).toBe(0);
    gate.resolve();
    await afterQueuedWork();
    expect(uploads.progress("s-1")?.fraction).toBeUndefined();
    await done;
  });

  it("moves with each advance, one publish for each, and is cleared by begin and by end", async () => {
    const uploads = createDropUploads();
    const seen: Array<number | undefined> = [];
    uploads.subscribe(() => seen.push(uploads.progress("s-1")?.fraction));
    await uploads.run("s-1", ["a.bin", "b.bin"], async (turn) => {
      turn.begin("a.bin");
      turn.advance(0, 8);
      turn.advance(2, 8);
      turn.advance(8, 8);
      turn.end();
      turn.begin("b.bin");
      turn.end();
    });
    const told = seen.filter((fraction) => fraction !== undefined);
    expect(told).toEqual([0, 0.25, 1]);
    // After a.bin ends the next file shows none until it says.
    expect(seen.at(-1)).toBeUndefined();
  });

  it("is cleared when the next file begins, though the last said it was complete", async () => {
    const uploads = createDropUploads();
    const gate = deferred();
    const done = uploads.run("s-1", ["a.bin", "b.bin"], async (turn) => {
      turn.begin("a.bin");
      turn.advance(5, 5);
      turn.end();
      turn.begin("b.bin");
      await gate.promise;
      turn.end();
    });
    expect(uploads.progress("s-1")).toMatchObject({ name: "b.bin", fraction: undefined });
    gate.resolve();
    await done;
  });
});

describe("telling of a change", () => {
  it("calls a listener at each change until it is removed", async () => {
    const uploads = createDropUploads();
    const listener = vi.fn();
    const stop = uploads.subscribe(listener);
    const { task, finish } = steppedTask(["a"]);
    const done = uploads.run("s-1", ["a"], task);
    expect(listener).toHaveBeenCalled();
    finish(0);
    await done;
    const calls = listener.mock.calls.length;
    stop();
    await uploads.run("s-2", ["b"], async (turn) => {
      turn.begin("b");
      turn.end();
    });
    expect(listener).toHaveBeenCalledTimes(calls);
  });
});

describe("running drops", () => {
  it("starts the task of a drop with none before it at once", () => {
    const uploads = createDropUploads();
    const task = vi.fn(async () => undefined);
    void uploads.run("s-1", ["a"], task);
    expect(task).toHaveBeenCalledOnce();
  });

  it("answers what the task answers", async () => {
    const uploads = createDropUploads();
    await expect(uploads.run("s-1", [], async () => "pasted")).resolves.toBe("pasted");
  });

  it("holds a drop made while another runs until that one is done, across sessions, and runs them in the order made", async () => {
    const uploads = createDropUploads();
    const order: string[] = [];
    const first = deferred();
    const second = deferred();
    const run = (session: string, label: string, gate?: { promise: Promise<void> }) =>
      uploads.run(session, [label], async (turn) => {
        order.push(`start ${label}`);
        turn.begin(label);
        await gate?.promise;
        turn.end();
        order.push(`end ${label}`);
      });
    const done = [run("s-1", "one", first), run("s-2", "two", second), run("s-1", "three")];
    await afterQueuedWork();
    expect(order).toEqual(["start one"]);
    first.resolve();
    await afterQueuedWork();
    expect(order).toEqual(["start one", "end one", "start two"]);
    second.resolve();
    await Promise.all(done);
    expect(order).toEqual(["start one", "end one", "start two", "end two", "start three", "end three"]);
  });

  it("goes on with the next drop where one fails, and still tells the failure to its own caller", async () => {
    const uploads = createDropUploads();
    const failing = deferred();
    const first = uploads.run("s-1", ["a"], async () => {
      await failing.promise;
      throw new Error("boom");
    });
    const second = uploads.run("s-1", ["b"], async () => "second ran");
    failing.resolve();
    await expect(first).rejects.toThrow("boom");
    await expect(second).resolves.toBe("second ran");
  });
});

describe("cancelling a session's uploads", () => {
  it("aborts the signal of a drop under way and of those queued behind it, for that session only", async () => {
    const uploads = createDropUploads();
    const signals: Record<string, AbortSignal> = {};
    const gate = deferred();
    const run = (session: string, label: string) =>
      uploads.run(session, [label], async (turn) => {
        signals[label] = turn.signal;
        await gate.promise;
      });
    const done = [run("s-1", "one"), run("s-1", "two"), run("s-2", "other")];
    uploads.cancel("s-1");
    expect(signals.one.aborted).toBe(true);
    gate.resolve();
    await Promise.all(done);
    expect(signals.two.aborted).toBe(true);
    expect(signals.other.aborted).toBe(false);
  });

  it("leaves a drop made after Cancel, while the cancelled drop is still finishing, to go on, and stops the ones queued before it", async () => {
    const uploads = createDropUploads();
    const signals: Record<string, AbortSignal> = {};
    const gate = deferred();
    const run = (label: string) =>
      uploads.run("s-1", [label], async (turn) => {
        signals[label] = turn.signal;
        await gate.promise;
      });
    const done = [run("running"), run("queued")];
    uploads.cancel("s-1");
    done.push(run("after"));
    gate.resolve();
    await Promise.all(done);
    expect(signals.running.aborted).toBe(true);
    expect(signals.queued.aborted).toBe(true);
    expect(signals.after.aborted).toBe(false);
  });

  it("does nothing for a session with nothing queued", () => {
    const uploads = createDropUploads();
    expect(() => uploads.cancel("s-none")).not.toThrow();
  });

  it("does not hold against drops made once the session's uploads are all done", async () => {
    const uploads = createDropUploads();
    const gate = deferred();
    const first = uploads.run("s-1", ["a"], async () => {
      await gate.promise;
    });
    uploads.cancel("s-1");
    gate.resolve();
    await first;
    let later: AbortSignal | undefined;
    await uploads.run("s-1", ["b"], async (turn) => {
      later = turn.signal;
    });
    expect(later?.aborted).toBe(false);
  });
});

describe("the counting after Cancel", () => {
  it("has no progress for a session whose cancelled drops are the only ones left, at once", async () => {
    const uploads = createDropUploads();
    const gate = deferred();
    const done = uploads.run("s-1", ["a", "b"], async (turn) => {
      turn.begin("a");
      await gate.promise;
      turn.end();
    });
    expect(uploads.progress("s-1")).toBeDefined();
    uploads.cancel("s-1");
    expect(uploads.progress("s-1")).toBeUndefined();
    gate.resolve();
    await done;
    expect(uploads.progress("s-1")).toBeUndefined();
  });

  it("counts a drop made after Cancel from nothing, as 1 of 1, while the cancelled drop is still finishing", async () => {
    const uploads = createDropUploads();
    const old = steppedTask(["old1", "old2"]);
    const fresh = steppedTask(["new1"]);
    const oldDone = uploads.run("s-1", ["old1", "old2"], old.task);
    uploads.cancel("s-1");
    void uploads.run("s-1", ["new1"], fresh.task);
    expect(uploads.progress("s-1")).toEqual({ index: 1, total: 1, name: "new1" });
    old.finish(0);
    old.finish(1);
    await oldDone;
    expect(uploads.progress("s-1")).toEqual({ index: 1, total: 1, name: "new1" });
  });

  it("lets the cancelled drops finish counting in their own group, which the new group's numbers and name never show", async () => {
    const uploads = createDropUploads();
    const gate = deferred();
    let oldTurn!: DropTurn;
    const oldDone = uploads.run("s-1", ["old1", "old2"], async (turn) => {
      oldTurn = turn;
      turn.begin("old1");
      await gate.promise;
      turn.end();
      turn.begin("old2");
      turn.advance(1, 2);
      turn.end();
    });
    uploads.cancel("s-1");
    const freshGate = deferred();
    const freshDone = uploads.run("s-1", ["new1", "new2"], async (turn) => {
      turn.begin("new1");
      await freshGate.promise;
      turn.end();
      turn.begin("new2");
      await freshGate.promise;
      turn.end();
    });
    expect(oldTurn).toBeDefined();
    gate.resolve();
    await oldDone;
    expect(uploads.progress("s-1")).toEqual({ index: 1, total: 2, name: "new1" });
    freshGate.resolve();
    await freshDone;
    expect(uploads.progress("s-1")).toBeUndefined();
  });

  it("goes on counting a drop for a session after its other drops were all cancelled and done", async () => {
    const uploads = createDropUploads();
    const gate = deferred();
    const first = uploads.run("s-1", ["a"], async () => {
      await gate.promise;
    });
    uploads.cancel("s-1");
    gate.resolve();
    await first;
    const next = steppedTask(["b"]);
    void uploads.run("s-1", ["b"], next.task);
    expect(uploads.progress("s-1")).toEqual({ index: 1, total: 1, name: "b" });
    next.finish(0);
  });
});

describe("whether a session has uploads queued", () => {
  it("is a boolean that renders a component only when it flips, not with every part", async () => {
    const renders = vi.fn();
    function Probe() {
      renders(useDropUploadsShown("s-shown-hook"));
      return null;
    }
    mountHost(<Probe />);
    expect(renders.mock.calls.map(([shown]) => shown)).toEqual([false]);
    const gate = deferred();
    let done!: Promise<void>;
    act(() => {
      done = dropUploads.run("s-shown-hook", ["a.bin", "b.bin"], async (turn) => {
        turn.begin("a.bin");
        turn.advance(0, 8);
        await gate.promise;
        turn.advance(2, 8);
        turn.advance(8, 8);
        turn.end();
        turn.begin("b.bin");
        turn.advance(0, 4);
        turn.advance(4, 4);
        turn.end();
      });
    });
    const afterAppearing = renders.mock.calls.length;
    expect(renders).toHaveBeenLastCalledWith(true);
    await act(async () => {
      gate.resolve();
      await done;
    });
    // Everything between the appearing and the going, parts and file changes included, rendered nothing.
    expect(renders.mock.calls.slice(afterAppearing).map(([shown]) => shown)).toEqual([false]);
  });

  it("is false for no session", () => {
    const seen = vi.fn();
    function Probe() {
      seen(useDropUploadsShown(undefined));
      return null;
    }
    mountHost(<Probe />);
    expect(seen).toHaveBeenLastCalledWith(false);
  });

  it("is false for a session once Cancel left it nothing to show", async () => {
    const seen = vi.fn();
    function Probe() {
      seen(useDropUploadsShown("s-cancel-hook"));
      return null;
    }
    mountHost(<Probe />);
    const gate = deferred();
    let done!: Promise<void>;
    act(() => {
      done = dropUploads.run("s-cancel-hook", ["a.bin"], async (turn) => {
        turn.begin("a.bin");
        await gate.promise;
        turn.end();
      });
    });
    expect(seen).toHaveBeenLastCalledWith(true);
    act(() => dropUploads.cancel("s-cancel-hook"));
    expect(seen).toHaveBeenLastCalledWith(false);
    await act(async () => {
      gate.resolve();
      await done;
    });
  });
});

describe("the progress a component shows", () => {
  function Probe({ session, seen }: { session: string | undefined; seen: (progress: DropProgress | undefined) => void }) {
    seen(useDropProgress(session));
    return null;
  }

  it("is none for no session", () => {
    const seen = vi.fn();
    mountHost(<Probe session={undefined} seen={seen} />);
    expect(seen).toHaveBeenLastCalledWith(undefined);
  });

  it("is none while nothing is queued for the session", () => {
    const seen = vi.fn();
    mountHost(<Probe session="s-never-queued" seen={seen} />);
    expect(seen).toHaveBeenLastCalledWith(undefined);
  });
});

describe("the window's one queue", () => {
  it("re-renders a component as the uploads of its session move", async () => {
    const seen = vi.fn();
    function Probe() {
      seen(useDropProgress("s-queue-hook"));
      return null;
    }
    mountHost(<Probe />);
    const gate = deferred();
    let done!: Promise<void>;
    act(() => {
      done = dropUploads.run("s-queue-hook", ["x.bin", "y.bin"], async (turn) => {
        turn.begin("x.bin");
        await gate.promise;
        turn.end();
        turn.begin("y.bin");
        turn.end();
      });
    });
    expect(seen).toHaveBeenLastCalledWith({ index: 1, total: 2, name: "x.bin" });
    await act(async () => {
      gate.resolve();
      await done;
    });
    expect(seen).toHaveBeenLastCalledWith(undefined);
  });
});
