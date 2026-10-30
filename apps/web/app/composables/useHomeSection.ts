import { featureGroups, homeSections } from "../../seo/page-outline";

export function useHomeSection() {
  const activeSection = ref<string>("overview");
  const activeGroup = ref<string>("");
  let frame = 0;
  const ids = homeSections.map((section) => section.id);
  const update = () => {
    frame = 0;
    const line = Math.min(window.innerHeight * 0.3, 220);
    let current: string = "overview";
    for (const id of ids) {
      const element = document.getElementById(id);
      if (element && element.getBoundingClientRect().top <= line) current = id;
    }
    if (window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 4) current = "download";
    activeSection.value = current;
    activeGroup.value = "";
    if (current === "benefits") {
      for (const group of featureGroups) {
        const element = document.getElementById(group.id);
        if (element && element.getBoundingClientRect().top <= line) activeGroup.value = group.id;
      }
    }
  };
  const schedule = () => { if (!frame) frame = requestAnimationFrame(update); };
  onMounted(() => {
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    update();
  });
  onBeforeUnmount(() => {
    window.removeEventListener("scroll", schedule);
    window.removeEventListener("resize", schedule);
    cancelAnimationFrame(frame);
  });
  return { activeSection, activeGroup };
}
