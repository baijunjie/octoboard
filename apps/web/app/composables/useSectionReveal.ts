export function useSectionReveal(container: Ref<HTMLElement | null>) {
  let observer: IntersectionObserver | undefined;
  onMounted(() => {
    if (
      !container.value ||
      matchMedia("(prefers-reduced-motion: reduce)").matches ||
      !("IntersectionObserver" in window)
    )
      return;
    observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-revealed");
            observer?.unobserve(entry.target);
          }
        }
      },
      { threshold: 0.12 },
    );
    for (const element of container.value.querySelectorAll("[data-reveal]")) {
      if (element.getBoundingClientRect().top > window.innerHeight)
        element.classList.add("reveal-ready");
      observer.observe(element);
    }
  });
  onBeforeUnmount(() => observer?.disconnect());
}
