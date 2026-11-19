import os from "node:os";
import path from "node:path";

import { afterEach, expect, onTestFinished, test, vi } from "vitest";

import { createRun, firstLine, launchBrowser, loadChromium, loadFrame, readGallery, READ_TIMEOUT_MS, SetupError, startGalleryServer, wildcard, within } from "./gallery-browser.mjs";

// The dev server is the real `vite` in the sweep's own tests; here it is replaced, so that what `startGalleryServer`
// does with a server (the address it reports, when the caller is handed it) can be asserted without starting one.
const serverHooks = vi.hoisted(() => ({ create: undefined }));
vi.mock("vite", () => ({ createServer: (config) => serverHooks.create(config) }));

afterEach(() => {
  serverHooks.create = undefined;
});

/** Fake timers for one test, run out and restored when it finishes, with nothing left armed. */
function fakeClock() {
  vi.useFakeTimers();
  onTestFinished(() => {
    vi.runOnlyPendingTimers();
    expect(vi.getTimerCount()).toBe(0);
    vi.useRealTimers();
  });
}

test("`within` settles as its promise does, and leaves no timer behind", async () => {
  fakeClock();
  await expect(within(5_000, Promise.resolve("done"), "reading")).resolves.toBe("done");
  expect(vi.getTimerCount()).toBe(0);
  await expect(within(5_000, Promise.reject(new Error("page crashed")), "reading")).rejects.toThrow("page crashed");
  expect(vi.getTimerCount()).toBe(0);
});

test("`within` rejects, naming what took too long and for how many seconds, once the limit passes", async () => {
  fakeClock();
  const outcome = within(5_000, new Promise(() => {}), "reading the page").then(
    () => "resolved",
    (error) => error.message,
  );
  await vi.advanceTimersByTimeAsync(4_999);
  await vi.advanceTimersByTimeAsync(1);
  await expect(outcome).resolves.toBe("reading the page took more than 5s");
});

test("`within` does not reject a promise that settles before the limit", async () => {
  fakeClock();
  let finish;
  const outcome = within(5_000, new Promise((resolve) => (finish = resolve)), "reading");
  await vi.advanceTimersByTimeAsync(4_999);
  finish("late but in time");
  await expect(outcome).resolves.toBe("late but in time");
});

function fakeBrowser({ evaluate = async () => ({ scenarios: [] }), goto = async () => {} } = {}) {
  const page = { goto: vi.fn(goto), evaluate: vi.fn(evaluate), close: vi.fn(async () => {}) };
  return { page, browser: { newPage: vi.fn(async () => page) } };
}

test("the gallery's scenarios are read from the frame page of the origin, and the page is closed", async () => {
  const gallery = { scenarios: [{ id: "a", live: ["x"] }], languages: ["en"], catalogLanguages: ["en"] };
  const { browser, page } = fakeBrowser({ evaluate: async () => gallery });
  await expect(readGallery(browser, "http://localhost:9")).resolves.toBe(gallery);
  expect(page.goto).toHaveBeenCalledWith(expect.stringMatching(/^http:\/\/localhost:9\/gallery-frame\.html\?/), expect.objectContaining({ timeout: READ_TIMEOUT_MS }));
  expect(page.close).toHaveBeenCalledTimes(1);
});

test("a page that cannot be loaded, or read, is an error from `readGallery`, and the page is still closed", async () => {
  for (const options of [{ goto: async () => { throw new Error("net::ERR_CONNECTION_REFUSED"); } }, { evaluate: async () => { throw new Error("import failed"); } }]) {
    const { browser, page } = fakeBrowser(options);
    await expect(readGallery(browser, "http://localhost:9")).rejects.toThrow(/ERR_CONNECTION_REFUSED|import failed/);
    expect(page.close).toHaveBeenCalledTimes(1);
  }
});

test("a page that never answers is given up on after the read timeout, naming the reading", async () => {
  fakeClock();
  const { browser, page } = fakeBrowser({ evaluate: () => new Promise(() => {}) });
  const outcome = readGallery(browser, "http://localhost:9").then(
    () => "resolved",
    (error) => error.message,
  );
  await vi.advanceTimersByTimeAsync(READ_TIMEOUT_MS);
  await expect(outcome).resolves.toBe(`reading the scenarios took more than ${READ_TIMEOUT_MS / 1000}s`);
  expect(page.close).toHaveBeenCalledTimes(1);
});

test("the dev server of the package is started on a free port and reports its origin, and closing it closes the server", async () => {
  const close = vi.fn(async () => {});
  let config;
  serverHooks.create = async (given) => {
    config = given;
    return { listen: async () => {}, httpServer: { address: () => ({ port: 4321 }) }, close };
  };
  const server = await startGalleryServer();
  expect(server.origin).toBe("http://localhost:4321");
  expect(config.root).toBe(path.resolve(import.meta.dirname, ".."));
  expect(config.server).toMatchObject({ port: 0, open: false });
  await server.close();
  expect(close).toHaveBeenCalledTimes(1);
});

test("the server is handed to the caller before it listens, so that a failed listen can still be closed", async () => {
  const events = [];
  const created = {
    async listen() {
      events.push("listen");
      throw new Error("listen failed");
    },
    httpServer: undefined,
    close: async () => {},
  };
  serverHooks.create = async () => created;
  await expect(startGalleryServer((server) => events.push(server === created ? "handed over" : "wrong server"))).rejects.toThrow("listen failed");
  expect(events).toEqual(["handed over", "listen"]);
});

test("a server that reports no port, or a pipe, is a setup error", async () => {
  for (const address of [null, "/tmp/pipe"]) {
    serverHooks.create = async () => ({ listen: async () => {}, httpServer: { address: () => address }, close: async () => {} });
    await expect(startGalleryServer()).rejects.toBeInstanceOf(SetupError);
  }
});

test("a Playwright directory that does not exist is a setup error naming it", async () => {
  const missing = path.join(os.tmpdir(), "gallery-browser-no-such-playwright");
  await expect(loadChromium(missing)).rejects.toThrow(SetupError);
  await expect(loadChromium(missing)).rejects.toThrow(missing);
});

test("the package's own Playwright is used when none is named, and it is reported as the package's own", async () => {
  const { chromium, own } = await loadChromium(undefined);
  expect(own).toBe(true);
  expect(typeof chromium.launch).toBe("function");
});

test("a browser named by path is launched at that path, with the signals left to the caller", async () => {
  const browser = {};
  const chromium = { launch: vi.fn(async () => browser) };
  await expect(launchBrowser(chromium, "/opt/chrome", true)).resolves.toBe(browser);
  expect(chromium.launch).toHaveBeenCalledWith(expect.objectContaining({ executablePath: "/opt/chrome", handleSIGINT: false, handleSIGTERM: false, handleSIGHUP: false }));
});

test("a browser named by path that will not launch is a setup error naming the path", async () => {
  const chromium = { launch: vi.fn(async () => { throw new Error("spawn ENOENT"); }) };
  await expect(launchBrowser(chromium, "/opt/chrome", true)).rejects.toThrow(SetupError);
  await expect(launchBrowser(chromium, "/opt/chrome", true)).rejects.toThrow("/opt/chrome");
});

test("a browser that will not start, though it is there, is a setup error that advises naming another", async () => {
  const chromium = { launch: vi.fn(async () => { throw new Error("libnss3.so: cannot open shared object file"); }) };
  const failure = launchBrowser(chromium, undefined, true);
  await expect(failure).rejects.toThrow(SetupError);
  await expect(launchBrowser(chromium, undefined, true)).rejects.toThrow(/Could not launch Chromium: libnss3.so/);
  await expect(launchBrowser(chromium, undefined, true)).rejects.toThrow(/--chromium/);
});

test("the launch limit is passed on when there is one", async () => {
  const chromium = { launch: vi.fn(async () => ({})) };
  await launchBrowser(chromium, "/opt/chrome", true, 1234);
  expect(chromium.launch).toHaveBeenCalledWith(expect.objectContaining({ timeout: 1234 }));
  await launchBrowser(chromium, undefined, true, 99);
  expect(chromium.launch).toHaveBeenLastCalledWith(expect.objectContaining({ timeout: 99 }));
  await launchBrowser(chromium, undefined, true);
  expect(chromium.launch.mock.lastCall[0]).not.toHaveProperty("timeout");
});

test.each([
  ["a*", "abc", true],
  ["*c", "abc", true],
  ["a*c", "ac", true],
  ["a*b*c", "axxbyyc", true],
  ["*", "", true],
  ["abc", "abcd", false],
  ["abc", "xabc", false],
  ["A", "a", false],
  ["a.c", "abc", false],
  ["a?c", "abc", false],
  ["a+", "aa", false],
  ["(a)", "a", false],
  ["[a]", "a", false],
  ["a|b", "a", false],
  ["^a$", "a", false],
  ["a.c", "a.c", true],
  ["a$b", "a$b", true],
  ["a\\b", "a\\b", true],
])("the wildcard %j matches %j: %s", (pattern, text, matches) => {
  expect(wildcard(pattern).test(text)).toBe(matches);
});

test.each([
  [new Error("first\nsecond"), "first"],
  ["plain\nlines", "plain"],
  [{ message: "from a message\nmore" }, "from a message"],
  [new Error(""), ""],
  [undefined, "undefined"],
  [null, "null"],
])("the first line of %j is %j", (error, line) => {
  expect(firstLine(error)).toBe(line);
});

function fakePage({ banner = [], waitForSelector = async () => {}, evaluate, goto = async () => {} } = {}) {
  const calls = [];
  const listeners = {};
  const page = {
    on: vi.fn((event, listener) => {
      listeners[event] = listener;
    }),
    goto: vi.fn(async (...args) => {
      calls.push("goto");
      await goto(listeners, ...args);
    }),
    waitForSelector: vi.fn(async (...args) => {
      calls.push("wait for ready");
      await waitForSelector(...args);
    }),
    waitForTimeout: vi.fn(async () => {
      calls.push("settle");
    }),
    evaluate: vi.fn(async () => {
      calls.push("read overlay");
      return evaluate ? evaluate() : banner;
    }),
  };
  return { page, calls, listeners };
}
const frame = { readyTimeoutMs: 60_000, settleMs: 300 };

test("a clean frame loads, is waited on until it says it is ready and has settled, is read, and has no problems", async () => {
  const { page, calls } = fakePage();
  await expect(loadFrame(page, "http://h/frame", frame)).resolves.toEqual([]);
  expect(calls).toEqual(["goto", "wait for ready", "settle", "read overlay"]);
  expect(page.goto).toHaveBeenCalledWith("http://h/frame", { waitUntil: "commit" });
  expect(page.waitForSelector).toHaveBeenCalledWith("html[data-gallery-ready]", { state: "attached", timeout: 60_000 });
  expect(page.waitForTimeout).toHaveBeenCalledWith(300);
});

test("the lines of the failure overlay come first, then the uncaught errors, the first line of each", async () => {
  const { page } = fakePage({ banner: ["fixture failed", "second"], goto: async (listeners) => listeners.pageerror(new Error("boom\n    at stack")) });
  await expect(loadFrame(page, "http://h/frame", frame)).resolves.toEqual(["fixture failed", "second", "uncaught: boom"]);
});

test("a frame that never says it is ready is a problem naming the wait, and the overlay is still read", async () => {
  const { page } = fakePage({ banner: ["fixture failed"], waitForSelector: async () => { throw Object.assign(new Error("Timeout 90000ms exceeded"), { name: "TimeoutError" }); } });
  await expect(loadFrame(page, "http://h/frame", { ...frame, readyTimeoutMs: 90_000 })).resolves.toEqual(["fixture failed", "the page never became ready within 90s"]);
  expect(page.waitForTimeout).toHaveBeenCalled();
});

test("a page that goes away under the wait is thrown as it is, not reported as a page that never became ready", async () => {
  const { page } = fakePage({ waitForSelector: async () => { throw new Error("Target page, context or browser has been closed"); } });
  await expect(loadFrame(page, "http://h/frame", frame)).rejects.toThrow("Target page, context or browser has been closed");
  expect(page.evaluate).not.toHaveBeenCalled();
});

test("an overlay that cannot be read is a problem naming why, not a thrown error", async () => {
  const { page } = fakePage({ evaluate: async () => { throw new Error("Execution context was destroyed\n    at stack"); } });
  await expect(loadFrame(page, "http://h/frame", frame)).resolves.toEqual(["could not read the page: Execution context was destroyed"]);
});

test("`afterSettle` runs once the page has settled and before the overlay is read, and the problems it adds are returned", async () => {
  const { page, calls } = fakePage();
  const afterSettle = vi.fn(async (problems) => {
    calls.push("after settle");
    problems.push("extra problem");
  });
  await expect(loadFrame(page, "http://h/frame", { ...frame, afterSettle })).resolves.toEqual(["extra problem"]);
  expect(calls).toEqual(["goto", "wait for ready", "settle", "after settle", "read overlay"]);
});

/** A run whose signal handlers are caught instead of registered, and whose exit and output are recorded. */
function newRun() {
  const handlers = {};
  const register = vi.spyOn(process, "on").mockImplementation((signal, handler) => {
    handlers[signal] = handler;
    return process;
  });
  const run = createRun();
  register.mockRestore();
  const exit = vi.spyOn(process, "exit").mockImplementation(() => undefined);
  const error = vi.spyOn(console, "error").mockImplementation(() => {});
  const exitCode = process.exitCode;
  onTestFinished(() => {
    exit.mockRestore();
    error.mockRestore();
    process.exitCode = exitCode;
  });
  return { run, handlers, exit, error };
}
const resource = () => ({ close: vi.fn(async () => {}) });

test("a run closes what it owns once, however often it is asked to", async () => {
  const { run } = newRun();
  const [browser, server] = [resource(), resource()];
  expect(run.own("Chromium", browser)).toBe(browser);
  run.own("Vite server", server);
  await run.close();
  await run.close();
  expect(browser.close).toHaveBeenCalledTimes(1);
  expect(server.close).toHaveBeenCalledTimes(1);
});

test("a resource that fails to close is reported by name, the others still close, and the run ends with exit code 2", async () => {
  const { run, exit, error } = newRun();
  const server = resource();
  run.own("Chromium", { close: async () => { throw new Error("boom\n    at stack"); } });
  run.own("Vite server", server);
  process.exitCode = undefined;
  await run.close();
  expect(error).toHaveBeenCalledWith("Cleanup failed for Chromium: boom");
  expect(server.close).toHaveBeenCalledTimes(1);
  await run.finish();
  expect(process.exitCode).toBe(2);
  expect(exit).toHaveBeenCalledWith(2);
});

test("a failed close does not turn the exit code of a failed run into 2", async () => {
  const { run, exit } = newRun();
  run.own("Chromium", { close: async () => { throw new Error("boom"); } });
  process.exitCode = 1;
  await run.close();
  await run.finish();
  expect(exit).toHaveBeenCalledWith(1);
});

test("a clean close leaves the end of the process to the caller", async () => {
  const { run, exit } = newRun();
  run.own("Chromium", resource());
  process.exitCode = 0;
  await run.close();
  await run.finish();
  expect(exit).not.toHaveBeenCalled();
});

test("a resource that never closes is given up on after the close timeout, naming it", async () => {
  fakeClock();
  const { run, error } = newRun();
  const other = resource();
  run.own("Vite server", { close: () => new Promise(() => {}) });
  run.own("Chromium", other);
  const closing = run.close();
  await vi.advanceTimersByTimeAsync(5_000);
  await closing;
  expect(error).toHaveBeenCalledWith("Cleanup failed for Vite server: closing Vite server took more than 5s");
  expect(other.close).toHaveBeenCalledTimes(1);
});

test.each([
  ["SIGHUP", 129],
  ["SIGINT", 130],
  ["SIGTERM", 143],
])("%s marks the run interrupted, closes what it owns and exits with code %i; a second one exits at once", async (signal, code) => {
  const { run, handlers, exit, error } = newRun();
  const browser = resource();
  run.own("Chromium", browser);
  expect(run.interrupted).toBe(false);
  handlers[signal]();
  expect(run.interrupted).toBe(true);
  expect(process.exitCode).toBe(code);
  expect(error).toHaveBeenCalledWith(`\n${signal}: stopping`);
  await vi.waitFor(() => expect(exit).toHaveBeenCalledWith(code));
  expect(browser.close).toHaveBeenCalledTimes(1);
  // `process.exit` does not return, so the mock that stands for it does not either.
  exit.mockClear();
  exit.mockImplementation(() => {
    throw new Error("exited");
  });
  expect(() => handlers[signal]()).toThrow("exited");
  expect(exit).toHaveBeenCalledOnce();
  expect(exit).toHaveBeenCalledWith(code);
  await run.finish();
  expect(exit).toHaveBeenCalledOnce();
});
