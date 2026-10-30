<script setup lang="ts">
const { t } = useSiteCopy();
const localePath = useLocalePath();
const { open, focusRequests, choose } = useAnalyticsConsent();
const panel = ref<HTMLElement | null>(null);
// The element focused before focus entered the prompt.
let before: HTMLElement | null = null;

// Reopened from the footer, even while it is still open from a first visit, the prompt takes focus
// so that it is read out and reached first. Shown on a first visit, it leaves focus where it is.
watch(focusRequests, async () => {
  await nextTick();
  panel.value?.focus();
});

function entered(event: FocusEvent) {
  const from = event.relatedTarget;
  if (from instanceof HTMLElement && !panel.value?.contains(from)) before = from;
}

// Whether the button being pressed with a pointer did not hold focus when the press began. A key
// press clears it, in case a pointer press before it never became a click.
let pressedUnfocused = false;
function pressing(event: PointerEvent) {
  pressedUnfocused = event.currentTarget !== document.activeElement;
}

// Not reopened, the prompt hands focus on only when the pressed button held it, as a keyboard or a
// screen reader's press does: back to where focus was before it entered the prompt, or, coming
// from nowhere, to the footer control that reopens it. A pointer press on a button that did not
// hold focus leaves focus and the view where they are.
function answer(consent: "granted" | "denied") {
  const fallback = document.querySelector<HTMLElement>("[data-analytics-settings]");
  const returnTo = pressedUnfocused ? null : before?.isConnected ? before : fallback;
  before = null;
  pressedUnfocused = false;
  choose(consent, returnTo);
}

// While the prompt covers the bottom of the window, the page scrolls clear of it: an element
// scrolled into view by keyboard focus stops above it, and the page's end, the footer, can scroll
// above it. The inset is the prompt's height plus its distance from the window's bottom edge and as
// much again of clearance.
let resize: ResizeObserver | undefined;
watch(panel, (element) => {
  resize?.disconnect();
  const root = document.documentElement;
  if (!element) {
    root.style.removeProperty("--analytics-consent-inset");
    return;
  }
  resize = new ResizeObserver(() => {
    const gap = window.innerHeight - element.getBoundingClientRect().bottom;
    root.style.setProperty("--analytics-consent-inset", `${element.offsetHeight + 2 * gap}px`);
  });
  resize.observe(element);
});
// Unmounted while shown, the prompt's own watcher never sees it go.
onBeforeUnmount(() => {
  resize?.disconnect();
  document.documentElement.style.removeProperty("--analytics-consent-inset");
});
</script>

<template>
  <section
    v-if="open"
    ref="panel"
    class="analytics-consent"
    :aria-label="t('consentLabel')"
    tabindex="-1"
    @focusin="entered"
  >
    <p>
      {{ t("consentBody")
      }}<NuxtLink class="text-link" :to="localePath('/privacy/')">{{
        t("privacyTitle")
      }}</NuxtLink>
    </p>
    <div class="analytics-consent-actions">
      <button
        type="button"
        class="consent-button"
        @pointerdown="pressing"
        @keydown="pressedUnfocused = false"
        @click="answer('granted')"
      >
        {{ t("consentAllow") }}
      </button>
      <button
        type="button"
        class="consent-button"
        @pointerdown="pressing"
        @keydown="pressedUnfocused = false"
        @click="answer('denied')"
      >
        {{ t("consentDecline") }}
      </button>
    </div>
  </section>
</template>
