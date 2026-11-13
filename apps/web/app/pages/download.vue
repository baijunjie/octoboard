<script setup lang="ts">
import {
  appPanelKeys,
  detectDownloadTab,
  downloadTabs,
  isDownloadTab,
  releasesUrl,
  serverDownloads,
  type DownloadTab,
  type Downloads,
} from "../../seo/downloads";

const { t } = useSiteCopy();
const config = useRuntimeConfig();
const route = useRoute();
const downloads = config.public.downloads as Downloads;
const allReleasesUrl = releasesUrl(config.public.repositoryUrl);
useSeoMeta({
  title: () => t("downloadSeoTitle"),
  description: () => t("downloadSeoDescription"),
  ogTitle: () => t("downloadSeoTitle"),
  ogDescription: () => t("downloadSeoDescription"),
});

// The three panels are all readable until the script has run; then they become tabs. The server render and the
// first client render therefore agree, and the visitor's system is only looked at after mounting.
const enhanced = ref(false);
const selected = ref<DownloadTab>("macos");
const tablist = ref<HTMLElement | null>(null);

function fromHash(): DownloadTab | undefined {
  // The router keeps the hash it was opened with, even where the address bar has dropped it.
  const id = (route.hash || window.location.hash).slice(1);
  return isDownloadTab(id) ? id : undefined;
}
function select(id: DownloadTab, focus = false) {
  selected.value = id;
  window.history.replaceState(window.history.state, "", `${window.location.pathname}${window.location.search}#${id}`);
  if (focus) {
    void nextTick(() => document.getElementById(`tab-${id}`)?.focus());
  }
}
function onKeys(event: KeyboardEvent) {
  const count = downloadTabs.length;
  const current = downloadTabs.findIndex((tab) => tab.id === selected.value);
  const rtl = tablist.value ? getComputedStyle(tablist.value).direction === "rtl" : false;
  const steps: Record<string, number> = {
    ArrowRight: rtl ? -1 : 1,
    ArrowLeft: rtl ? 1 : -1,
  };
  let next: number;
  if (event.key in steps) next = current + steps[event.key]!;
  else if (event.key === "Home") next = 0;
  else if (event.key === "End") next = count - 1;
  else return;
  event.preventDefault();
  const tab = downloadTabs[(next + count) % count];
  if (tab) select(tab.id, true);
}
function onHashChange() {
  const id = fromHash();
  if (id) selected.value = id;
}
watch(() => route.hash, onHashChange);
onMounted(() => {
  const platform = (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ?? "";
  selected.value = fromHash() ?? detectDownloadTab(platform, navigator.userAgent);
  enhanced.value = true;
  window.addEventListener("hashchange", onHashChange);
});
onBeforeUnmount(() => window.removeEventListener("hashchange", onHashChange));
</script>

<template>
  <main id="main-content" class="download-page wrap">
    <header class="page-heading">
      <h1>{{ t("downloadPageTitle") }}</h1>
      <p class="page-intro">{{ t("downloadPageSummary") }}</p>
    </header>
    <div
      v-if="enhanced"
      ref="tablist"
      class="download-tabs"
      role="tablist"
      :aria-label="t('downloadTabsLabel')"
      @keydown="onKeys"
    >
      <button
        v-for="tab in downloadTabs"
        :id="`tab-${tab.id}`"
        :key="tab.id"
        class="download-tab"
        type="button"
        role="tab"
        :aria-selected="selected === tab.id"
        :aria-controls="tab.id"
        :tabindex="selected === tab.id ? 0 : -1"
        @click="select(tab.id)"
      ><SiteIcon :name="tab.icon" />{{ tab.name }}</button>
    </div>

    <section
      v-for="tab in downloadTabs"
      :id="tab.id"
      :key="tab.id"
      class="download-panel"
      :role="enhanced ? 'tabpanel' : undefined"
      :aria-labelledby="enhanced ? `tab-${tab.id}` : undefined"
      :hidden="enhanced && selected !== tab.id"
      :tabindex="enhanced ? 0 : undefined"
    >
      <div class="download-card" :class="{ 'download-linux': tab.id === 'linux' }">
        <div class="download-card-head" :class="{ 'sr-only': enhanced }">
          <SiteIcon :name="tab.icon" />
          <h2>{{ tab.id === "linux" ? t("downloadLinuxTitle") : tab.name }}</h2>
        </div>
        <template v-if="tab.id !== 'linux'">
          <a :href="downloads[tab.id]" class="button">{{ t("downloadButton", { platform: tab.name }) }}</a>
          <p v-for="key in appPanelKeys[tab.id]" :key="key">{{ t(key) }}</p>
        </template>
        <template v-else>
          <p>{{ t("downloadLinuxDescription") }}</p>
          <p>{{ t("downloadLinuxRange") }}</p>
          <InstallCommand />
          <CommandMessage keypath="downloadServerRun" />
          <CommandMessage keypath="downloadPathNote" />
          <CommandMessage keypath="downloadServiceSteps" />
          <CommandMessage keypath="downloadUpgrade" />
          <p class="download-archives">
            <span>{{ t("downloadArchivesLabel") }}</span>
            <a
              v-for="target in serverDownloads"
              :key="target.id"
              :href="downloads[target.id]"
              class="text-link"
            >{{ t("downloadArchive", { architecture: target.architecture }) }}</a>
          </p>
          <TerminalDemo />
        </template>
      </div>
    </section>

    <a :href="allReleasesUrl" class="text-link download-releases"
    >{{ t("downloadAllReleases") }}<span class="link-arrow" aria-hidden="true"
    /></a>
  </main>
</template>
