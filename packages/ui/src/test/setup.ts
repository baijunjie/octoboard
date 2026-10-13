// jsdom has no ResizeObserver, which HeroUI's ScrollShadow and `FadeOverflow` need to mount, and
// `Dialog` puts a ScrollShadow around every dialog's body. A test that has to measure installs its
// own with `vi.stubGlobal`, which replaces this one.
class StubResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
globalThis.ResizeObserver ??= StubResizeObserver as unknown as typeof ResizeObserver;
