import type { Ref } from "vue";

/**
 * Pointer coordinates use the element's untransformed space and update at most once per frame.
 * Ignore events from overflowing controls outside the pill; touch receives pressed feedback only.
 */
export function useLiquidGlassPointer(target: Ref<HTMLElement | null>) {
  let motion: MediaQueryList | undefined;
  let hovering = false;
  let pressedPointer: number | null = null;
  let frame = 0;
  let pending: { x: number; y: number } | null = null;
  let detach: (() => void) | null = null;

  function within(el: HTMLElement, clientX: number, clientY: number) {
    const rect = el.getBoundingClientRect();
    return (
      clientX >= rect.left &&
      clientX <= rect.right &&
      clientY >= rect.top &&
      clientY <= rect.bottom
    );
  }

  function queuePosition(el: HTMLElement, clientX: number, clientY: number) {
    if (motion?.matches) return;
    pending = { x: clientX, y: clientY };
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (!pending) return;
      const rect = el.getBoundingClientRect();
      const scale = el.offsetWidth ? rect.width / el.offsetWidth : 1;
      el.style.setProperty(
        "--glass-x",
        `${((pending.x - rect.left) / scale).toFixed(1)}px`,
      );
      el.style.setProperty(
        "--glass-y",
        `${((pending.y - rect.top) / scale).toFixed(1)}px`,
      );
      pending = null;
    });
  }

  function clearPosition(el: HTMLElement) {
    if (frame) cancelAnimationFrame(frame);
    frame = 0;
    pending = null;
    el.style.removeProperty("--glass-x");
    el.style.removeProperty("--glass-y");
  }

  function setHover(el: HTMLElement, on: boolean) {
    if (hovering === on) return;
    hovering = on;
    el.classList.toggle("is-hover", on);
    if (!on && pressedPointer === null) clearPosition(el);
  }

  function track(this: HTMLElement, event: PointerEvent) {
    const inside = within(this, event.clientX, event.clientY);
    if (event.pointerType !== "touch") setHover(this, inside);
    if (inside && (hovering || pressedPointer !== null))
      queuePosition(this, event.clientX, event.clientY);
  }

  function onLeave(this: HTMLElement) {
    setHover(this, false);
  }

  function onDown(this: HTMLElement, event: PointerEvent) {
    if (event.button !== 0 || !within(this, event.clientX, event.clientY))
      return;
    pressedPointer = event.pointerId;
    this.classList.add("is-pressed");
    queuePosition(this, event.clientX, event.clientY);
  }

  function release(el: HTMLElement, event: PointerEvent) {
    if (event.pointerId !== pressedPointer) return;
    pressedPointer = null;
    el.classList.remove("is-pressed");
    if (!hovering) clearPosition(el);
  }

  onMounted(() => {
    const el = target.value;
    if (!el) return;
    motion = matchMedia("(prefers-reduced-motion: reduce)");
    const onMotionChange = () => clearPosition(el);
    motion.addEventListener("change", onMotionChange);
    const onRelease = (event: PointerEvent) => release(el, event);
    el.addEventListener("pointerenter", track);
    el.addEventListener("pointermove", track);
    el.addEventListener("pointerleave", onLeave);
    el.addEventListener("pointerdown", onDown);
    window.addEventListener("pointerup", onRelease);
    window.addEventListener("pointercancel", onRelease);
    detach = () => {
      motion?.removeEventListener("change", onMotionChange);
      el.removeEventListener("pointerenter", track);
      el.removeEventListener("pointermove", track);
      el.removeEventListener("pointerleave", onLeave);
      el.removeEventListener("pointerdown", onDown);
      window.removeEventListener("pointerup", onRelease);
      window.removeEventListener("pointercancel", onRelease);
      clearPosition(el);
      el.classList.remove("is-hover", "is-pressed");
    };
  });

  onBeforeUnmount(() => {
    detach?.();
    detach = null;
  });
}
