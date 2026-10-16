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
