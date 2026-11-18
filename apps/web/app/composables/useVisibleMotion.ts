/** Run decorative motion only while its surface is visible and motion is allowed. */
export function useVisibleMotion(target: Ref<HTMLElement | null>) {
  const running = ref(false);
  const reduced = ref(false);
  let observer: IntersectionObserver | undefined;
  let preference: MediaQueryList | undefined;
  let visible = false;

  const update = () => {
    reduced.value = preference?.matches ?? false;
    running.value = visible && !document.hidden && !reduced.value;
  };
  onMounted(() => {
    preference = matchMedia("(prefers-reduced-motion: reduce)");
    preference.addEventListener("change", update);
    document.addEventListener("visibilitychange", update);
    if ("IntersectionObserver" in window && target.value) {
      observer = new IntersectionObserver(([entry]) => {
        visible = entry?.isIntersecting ?? false;
        update();
      });
      observer.observe(target.value);
    } else {
      visible = true;
    }
    update();
  });
  onBeforeUnmount(() => {
    observer?.disconnect();
    preference?.removeEventListener("change", update);
    document.removeEventListener("visibilitychange", update);
  });
  return { running, reduced };
}
