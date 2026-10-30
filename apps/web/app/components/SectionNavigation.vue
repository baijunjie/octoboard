<script setup lang="ts">
import { homeSections } from "../../seo/page-outline";

const { t } = useSiteCopy();
const { activeSection, activeGroup } = useHomeSection();
const currentKey = computed(() => homeSections.find((section) => section.id === activeSection.value)?.key || "navOverview");
const open = ref(false);
const rail = ref<HTMLElement | null>(null);
const menu = ref<HTMLElement | null>(null);
const panel = ref<HTMLElement | null>(null);
const toggle = ref<HTMLButtonElement | null>(null);
async function show() {
  open.value = true;
  await nextTick();
  panel.value?.querySelector<HTMLAnchorElement>("a")?.focus();
}
function panelKeys(event: KeyboardEvent) {
  if (event.key !== "Tab") return;
  const links = panel.value?.querySelectorAll<HTMLAnchorElement>("a");
  if (!links?.length) return;
  if (event.shiftKey && document.activeElement === links[0]) {
    event.preventDefault();
    close(true);
  } else if (!event.shiftKey && document.activeElement === links[links.length - 1]) {
    event.preventDefault();
    close();
    menu.value?.closest(".nav-actions")?.querySelector<HTMLSelectElement>("select")?.focus();
  }
}
function resize() {
  if (window.innerWidth < 1440 && rail.value?.contains(document.activeElement)) {
    close(true);
    return;
  }
  if (window.innerWidth >= 1440 && open.value) {
    const hadFocus = contains(document.activeElement);
    close();
    if (hadFocus) navigate(activeSection.value);
  }
}
function close(restoreFocus = false) {
  open.value = false;
  if (restoreFocus) toggle.value?.focus();
}
function contains(target: EventTarget | null) {
  return target instanceof Node && (menu.value?.contains(target) || panel.value?.contains(target));
}
function outside(event: PointerEvent) {
  if (!contains(event.target)) close();
}
function focusOut(event: FocusEvent) {
  if (!contains(event.relatedTarget)) close();
}
function navigate(id: string) {
  close();
  const target = document.getElementById(id);
  if (target) {
    target.setAttribute("tabindex", "-1");
    target.focus({ preventScroll: true });
  }
}
onMounted(() => {
  document.addEventListener("pointerdown", outside);
  window.addEventListener("resize", resize);
});
onBeforeUnmount(() => {
  document.removeEventListener("pointerdown", outside);
  window.removeEventListener("resize", resize);
});
</script>

<template>
  <Teleport to="body">
    <nav ref="rail" class="section-rail" :aria-label="t('pageContents')">
      <p>{{ t("pageContents") }}</p>
      <SectionLinks :active-section="activeSection" :active-group="activeGroup" @navigate="navigate" />
    </nav>
  </Teleport>
  <div ref="menu" class="section-menu" @keydown.esc.prevent="close(true)" @focusout="focusOut">
    <button ref="toggle" class="section-menu-toggle" type="button" :aria-expanded="open" aria-controls="page-directory" @click="open ? close() : show()" @keydown.down.prevent="show()">
      <span class="section-menu-label">{{ t("pageContents") }}</span>
      <span class="section-menu-current">{{ t(currentKey) }}</span>
      <span class="section-menu-chevron" aria-hidden="true" />
    </button>
    <Teleport to="body">
      <nav v-if="open" id="page-directory" ref="panel" class="section-menu-panel" :aria-label="t('pageContents')" @keydown.esc.prevent="close(true)" @focusout="focusOut" @keydown="panelKeys">
        <SectionLinks :active-section="activeSection" :active-group="activeGroup" expanded @navigate="navigate" />
      </nav>
    </Teleport>
  </div>
</template>

<style scoped>
.section-rail { z-index: 25; display: none; pointer-events: auto; position: fixed; inset-inline-end: 20px; top: 50%; transform: translateY(-50%); width: 176px; max-height: calc(100dvh - 224px); overflow-y: auto; overscroll-behavior: contain; scrollbar-width: thin; }
.section-rail > p { padding: 0 14px 16px; color: var(--muted); font-size: 11px; letter-spacing: 0.08em; }
.section-menu { position: relative; }
.section-menu-toggle { display: flex; align-items: center; gap: 10px; min-height: 44px; cursor: pointer; border: 0; padding: 0; background: transparent; font-size: 13px; color: var(--text); }
.section-menu-toggle::-webkit-details-marker { display: none; }
.section-menu-current { color: var(--muted); max-width: 128px; overflow-wrap: anywhere; }
.section-menu-chevron { width: 12px; height: 12px; flex-shrink: 0; background: currentColor; mask: url("../assets/icons/caret-down.svg") center / contain no-repeat; }
.section-menu-toggle[aria-expanded="true"] .section-menu-chevron { transform: rotate(180deg); }
.section-menu-panel { z-index: 35; position: fixed; top: 104px; inset-inline-end: 24px; width: min(380px, calc(100% - 48px)); max-height: calc(100dvh - 128px); overflow-y: auto; overscroll-behavior: contain; padding: 20px; border: 1px solid #454545; border-radius: 16px; background: #141414; box-shadow: 0 20px 60px #0009; }
@media (min-width: 1440px) { .section-rail { display: block; } .section-menu, .section-menu-panel { display: none; } }
@media (max-width: 700px) { .section-menu-current { display: none; } .section-menu-toggle { gap: 6px; font-size: 12px; } .section-menu-label { max-width: 76px; overflow-wrap: anywhere; } .section-menu-panel { top: 88px; inset-inline-end: 16px; width: calc(100% - 32px); max-height: calc(100dvh - 104px); } }
</style>
