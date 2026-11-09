import { beforeEach, onTestFinished } from "vitest";

// jsdom has no ResizeObserver, which HeroUI's ScrollShadow and `FadeOverflow` need to mount, and
// `Dialog` puts a ScrollShadow around every dialog's body. A test that has to measure installs its
// own with `vi.stubGlobal`, which replaces this one.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;

// jsdom has no Web Animations API, which react-aria's tab indicator asks for as the chosen tab changes.
if (typeof Element !== "undefined") Element.prototype.getAnimations ??= () => [];

// Register first so this runs after every afterEach and every later onTestFinished, including unmount and timer drain.
if (typeof document !== "undefined") {
  // Bind after test mocks but before a test can reset modules, so cleanup keeps the static imports' queue.
  // Resolve once per file to avoid reinstalling focus/input listeners after module resets.
  let drain: (() => void) | undefined;
  beforeEach(async () => {
    drain ??= (await import("./alertQueue")).acknowledgeAllAlerts;
    onTestFinished(() => drain!());
  });
}
