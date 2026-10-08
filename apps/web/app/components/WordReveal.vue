<script setup lang="ts">
const props = defineProps<{ text: string }>();
const { locale } = useI18n();
const container = ref<HTMLElement | null>(null);
let observer: IntersectionObserver | undefined;
const lines = computed(() => {
  const segmenter = new Intl.Segmenter(locale.value, { granularity: "word" });
  let order = 0;
  return props.text.split("\n").map((line) =>
    Array.from(segmenter.segment(line)).map((segment) => ({
      text: segment.segment,
      order: order++,
    })),
  );
});
async function observe() {
  observer?.disconnect();
  await nextTick();
  if (
    !container.value ||
    matchMedia("(prefers-reduced-motion: reduce)").matches ||
    !("IntersectionObserver" in window)
  )
    return;
  container.value.classList.add("word-reveal-ready");
  observer = new IntersectionObserver(
    (entries) => {
      entries
        .filter((entry) => entry.isIntersecting)
        .sort(
          (a, b) =>
            Number((a.target as HTMLElement).dataset.word) -
            Number((b.target as HTMLElement).dataset.word),
        )
        .forEach((entry, index) => {
          (entry.target as HTMLElement).style.setProperty(
            "--word-delay",
            `${index * 64}ms`,
          );
          entry.target.classList.add("is-word-visible");
          observer?.unobserve(entry.target);
        });
    },
    { rootMargin: "0px 0px -35% 0px", threshold: 0 },
  );
  for (const word of container.value.querySelectorAll("[data-word]"))
    observer.observe(word);
}
onMounted(observe);
watch(() => props.text, observe);
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <p ref="container" class="word-reveal">
    <span class="sr-only">{{ text }}</span>
    <span
      v-for="(line, index) in lines"
      :key="index"
      class="reveal-line"
      aria-hidden="true"
      ><span
        v-for="(word, wordIndex) in line"
        :key="wordIndex"
        :data-word="word.order"
        >{{ word.text }}</span
      ></span
    >
  </p>
</template>
