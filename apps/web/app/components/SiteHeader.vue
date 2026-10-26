<script setup lang="ts">
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const localePath = useLocalePath();
const route = useRoute();
const open = ref(false);
const compactNav = ref(true);
const activeSection = ref("");
const menuToggle = ref<HTMLButtonElement | null>(null);
const menuPanel = ref<HTMLElement | null>(null);
const glass = ref<HTMLElement | null>(null);
useLiquidGlass(glass);
useLiquidGlassPointer(glass);
const sections = [
  { id: "benefits", key: "navContinuity" },
  { id: "product-preview", key: "previewEyebrow" },
  { id: "how-it-works", key: "navHow" },
  { id: "developer-intent", key: "developerEyebrow" },
  { id: "local-workspace", key: "localEyebrow" },
  { id: "questions", key: "navFaq" },
];
let previousOverflow = "";
let sectionObserver: IntersectionObserver | undefined;
let navObserver: ResizeObserver | undefined;
const releasesUrl = `${config.public.repositoryUrl}/releases`;
function observeNavWidth() {
  const pill = glass.value;
  const header = pill?.parentElement;
  const nav = pill?.querySelector<HTMLElement>(".desktop-nav");
  const brand = pill?.querySelector<HTMLElement>(".brand");
  const actions = pill?.querySelector<HTMLElement>(".nav-actions");
  if (!pill || !header || !nav || !brand || !actions || !("ResizeObserver" in window)) return;
  const update = () => {
    const headerStyle = getComputedStyle(header);
    const pillStyle = getComputedStyle(pill);
    const available = header.clientWidth
      - parseFloat(headerStyle.paddingInlineStart) - parseFloat(headerStyle.paddingInlineEnd);
    const needed = brand.offsetWidth + nav.scrollWidth + actions.scrollWidth
      + 2 * parseFloat(pillStyle.columnGap)
      + parseFloat(pillStyle.paddingInlineStart) + parseFloat(pillStyle.paddingInlineEnd)
      + parseFloat(pillStyle.borderInlineStartWidth) + parseFloat(pillStyle.borderInlineEndWidth);
    const collapse = getComputedStyle(nav).display === "none" || Math.ceil(needed) > available;
    if (collapse && !compactNav.value && nav.contains(document.activeElement)) menuToggle.value?.focus();
    compactNav.value = collapse;
  };
  navObserver = new ResizeObserver(update);
  // Measure the labels even while collapsed so font and language changes can restore the inline links.
  for (const element of [header, brand, actions, ...nav.children]) navObserver.observe(element);
  update();
}
async function showMenu() {
  if (open.value) return;
  previousOverflow = document.body.style.overflow;
  document.body.style.overflow = "hidden";
  open.value = true;
  await nextTick();
  menuToggle.value?.focus();
  document.getElementById("site-shell")?.setAttribute("inert", "");
}
async function closeMenu(restoreFocus = true) {
  if (!open.value) return;
  open.value = false;
  document.getElementById("site-shell")?.removeAttribute("inert");
  document.body.style.overflow = previousOverflow;
  if (restoreFocus) {
    await nextTick();
    menuToggle.value?.focus();
  }
}
function trapFocus(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    closeMenu();
    return;
  }
  if (event.key !== "Tab") return;
  const focusable = [
    menuToggle.value,
    ...Array.from(
      menuPanel.value?.querySelectorAll<HTMLElement>(
        "a[href], button, select",
      ) || [],
    ),
  ].filter((element): element is HTMLElement => element !== null);
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}
async function observeSections() {
  sectionObserver?.disconnect();
  activeSection.value = "";
  await nextTick();
  if (!("IntersectionObserver" in window)) return;
  sectionObserver = new IntersectionObserver(
    (entries) => {
      for (const entry of entries)
        if (entry.isIntersecting) activeSection.value = entry.target.id;
    },
    { rootMargin: "-15% 0px -55% 0px", threshold: 0 },
  );
  const hero = document.querySelector("main > .hero");
  if (hero) sectionObserver.observe(hero);
  for (const section of sections) {
    const element = document.getElementById(section.id);
    if (element) sectionObserver.observe(element);
  }
}
watch(
  () => route.fullPath,
  () => {
    closeMenu(false);
    observeSections();
  },
);
onMounted(observeSections);
onMounted(observeNavWidth);
onBeforeUnmount(() => {
  sectionObserver?.disconnect();
  navObserver?.disconnect();
  closeMenu(false);
});
</script>

<template>
  <a class="skip-link" href="#main-content">{{ t("skipContent") }}</a>
  <header class="site-header">
    <div ref="glass" class="nav-pill liquid-glass" :class="{ 'nav-compact': compactNav }">
      <NuxtLink
        :to="localePath('/')"
        class="brand"
        :aria-label="config.public.appName"
      >
        <img
          :src="`${config.app.baseURL}wordmark.png`"
          alt=""
          width="113"
          height="19"
        />
      </NuxtLink>
      <nav class="desktop-nav" :aria-label="t('navMenu')" :inert="compactNav" :aria-hidden="compactNav || undefined">
        <NuxtLink
          v-for="section in sections"
          :key="section.id"
          :to="`${localePath('/')}#${section.id}`"
          :aria-current="activeSection === section.id ? 'location' : undefined"
          :class="{ active: activeSection === section.id }"
        >{{ t(section.key) }}</NuxtLink
        >
      </nav>
      <div class="nav-actions">
        <LanguageSelect />
        <a :href="releasesUrl" class="button nav-cta"
        ><SiteIcon name="download-simple" />{{ t("heroCta") }}</a
        >
        <Teleport to="body" :disabled="!open">
          <div
            class="menu-control"
            :role="open ? 'dialog' : undefined"
            :aria-modal="open ? true : undefined"
            :aria-label="open ? t('navMenu') : undefined"
            @keydown="open && trapFocus($event)"
          >
            <button
              ref="menuToggle"
              class="menu-toggle"
              :class="{ 'is-open': open, 'menu-floating-close': open }"
              type="button"
              :aria-label="t(open ? 'navClose' : 'navOpen')"
              :aria-expanded="open"
              aria-controls="site-menu"
              @click="open ? closeMenu() : showMenu()"
            >
              <span /><span />
            </button>
            <Transition name="menu">
              <div
                v-if="open"
                id="site-menu"
                ref="menuPanel"
                class="menu-overlay"
              >
                <div class="menu-top">
                  <NuxtLink
                    :to="localePath('/')"
                    class="brand"
                    @click="closeMenu(false)"
                  ><img
                    :src="`${config.app.baseURL}wordmark.png`"
                    :alt="config.public.appName"
                    width="136"
                    height="23" /></NuxtLink
                  ><span class="menu-close-space" aria-hidden="true" />
                </div>
                <nav class="overlay-links" :aria-label="t('navMenu')">
                  <NuxtLink
                    v-for="(section, index) in sections"
                    :key="section.id"
                    :to="`${localePath('/')}#${section.id}`"
                    :style="{ '--link-order': index }"
                    :aria-current="
                      activeSection === section.id ? 'location' : undefined
                    "
                    @click="closeMenu(false)"
                  ><span class="menu-number">0{{ index + 1 }}</span
                  ><span class="menu-label">{{ t(section.key) }}</span><SiteIcon
                    name="arrow-right"
                    class="menu-section-arrow" /></NuxtLink
                  ><a
                    :href="config.public.repositoryUrl"
                    :style="{ '--link-order': sections.length }"
                  >
                    GitHub<span
                      class="link-arrow"
                      aria-hidden="true"
                    /></a>
                </nav>
                <div class="menu-bottom">
                  <LanguageSelect /><a :href="releasesUrl" class="button"
                  ><SiteIcon name="download-simple" />{{ t("heroCta") }}</a
                  >
                </div>
              </div>
            </Transition>
          </div>
        </Teleport>
      </div>
    </div>
  </header>
</template>
