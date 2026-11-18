<script setup lang="ts">
import { remoteGuides, remoteTopics } from "../../seo/page-outline";

const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
const route = useRoute();
const localePath = useLocalePath();
const guides = remoteGuides(config.public.siteUrl);
const sections = [...guides, ...remoteTopics];
const contents = ref<HTMLDetailsElement | null>(null);
const numbers = computed(() => new Intl.NumberFormat(locale.value));
const leadSpace = computed(() => ["zh", "zh-Hant", "ja"].includes(locale.value) ? "" : " ");

function followSection(id: string) {
  if (contents.value?.open) contents.value.open = false;
  document.getElementById(id)?.focus({ preventScroll: true });
}

useSeoMeta({
  title: () => t("remoteSeoTitle"),
  description: () => t("remoteIntro"),
  ogTitle: () => t("remoteSeoTitle"),
  ogDescription: () => t("remoteIntro"),
});
</script>

<template>
  <main id="main-content" class="remote-page wrap">
    <NuxtLink class="text-link privacy-back" :to="localePath('/')">
      <span aria-hidden="true">←</span> {{ t("privacyBack") }}
    </NuxtLink>
    <header class="page-heading">
      <h1>{{ t("remoteEyebrow") }}</h1>
      <p class="page-intro">{{ t("remoteIntro") }}</p>
    </header>
    <RemoteRoute />
    <div class="guide-layout">
      <div class="guide-directory">
        <details ref="contents">
          <summary aria-controls="remote-directory">{{ t("pageContents") }}</summary>
        </details>
        <nav id="remote-directory" :aria-label="t('pageContents')">
          <p aria-hidden="true">{{ t("pageContents") }}</p>
          <ul>
            <li v-for="section in sections" :key="section.id">
              <NuxtLink
                :to="{ hash: `#${section.id}` }"
                :aria-current="route.hash === `#${section.id}` ? 'location' : undefined"
                @click="followSection(section.id)"
              >{{ t(section.key) }}</NuxtLink>
            </li>
          </ul>
        </nav>
      </div>

      <div class="guide-body">
        <section v-for="guide in guides" :id="guide.id" :key="guide.id" class="guide-section" tabindex="-1">
          <h2>{{ t(guide.key) }}</h2>
          <ol class="guide-steps" role="list">
            <li v-for="(step, index) in guide.steps" :key="step.key">
              <span class="guide-number" aria-hidden="true">{{ numbers.format(index + 1) }}</span>
              <div class="step-content">
                <p><RemoteMessage :keypath="step.key" /></p>
                <InstallCommand v-if="'command' in step" :command="step.command" :label="step.command" />
                <div v-if="'preview' in step" class="guide-preview">
                  <FeatureAppView feature="remoteDevices" :title="t('remoteEyebrow')" />
                </div>
                <template v-if="'notes' in step">
                  <p v-for="note in step.notes" :key="note" class="guide-note"><RemoteMessage :keypath="note" /></p>
                </template>
              </div>
            </li>
          </ol>
          <p v-if="'note' in guide" class="guide-note browser-note"><RemoteMessage :keypath="guide.note" /></p>
        </section>

        <section v-for="topic in remoteTopics" :id="topic.id" :key="topic.id" class="guide-section" tabindex="-1">
          <h2>{{ t(topic.key) }}</h2>
          <ul class="guide-list">
            <li v-for="item in topic.items" :key="item">
              <strong>{{ t(`${item}Lead`) }}</strong>{{ leadSpace }}<RemoteMessage :keypath="item" />
            </li>
          </ul>
        </section>
      </div>
    </div>
  </main>
</template>

<style scoped>
.remote-page { max-width: 960px; margin-top: 24px; padding-block: 96px; }
.page-heading h1 { margin-top: 0; overflow-wrap: anywhere; }
.guide-layout {
  display: grid;
  grid-template-columns: minmax(145px, 185px) minmax(0, 1fr);
  gap: clamp(24px, 5vw, 64px);
  margin-top: 48px;
}
.guide-directory { position: sticky; top: 120px; align-self: start; max-height: calc(100dvh - 144px); overflow-y: auto; }
.guide-directory details { display: none; }
.guide-directory p { margin-bottom: 12px; color: var(--muted); font-size: 13px; }
.guide-directory ul { margin: 0; list-style: none; padding: 0; }
.guide-directory a {
  display: block;
  padding: 8px 12px;
  border-inline-start: 1px solid var(--edge);
  color: var(--muted);
  font-size: 14px;
  line-height: 1.5;
}
.guide-directory a:hover, .guide-directory a[aria-current] {
  color: var(--text);
  border-color: var(--red);
  background: #ff4b3e0c;
}
.guide-section { scroll-margin-top: 116px; }
.guide-section + .guide-section { margin-top: 44px; padding-top: 32px; border-top: 1px solid var(--edge); }
.guide-section h2 { font-size: clamp(22px, 3vw, 28px); font-weight: 500; line-height: 1.3; margin-bottom: 24px; }
.guide-steps { padding: 0; margin: 0; list-style: none; }
.guide-steps > li {
  display: grid;
  position: relative;
  grid-template-columns: 28px minmax(0, 1fr);
  gap: 14px;
  padding-bottom: 24px;
}
.guide-steps > li:last-child { padding-bottom: 0; }
.guide-steps > li:not(:last-child)::before {
  content: "";
  position: absolute;
  inset-block: 28px 0;
  inset-inline-start: 13.5px;
  width: 1px;
  background: #5b3b37;
}
.guide-number {
  display: grid;
  place-items: center;
  align-self: start;
  width: 28px;
  height: 28px;
  border: 1px solid #80534c;
  border-radius: 50%;
  background: #211716;
  color: #f4b0a6;
  font-size: 13px;
}
.step-content, .guide-list { color: #b8b8b8; font-size: 16px; line-height: 1.75; overflow-wrap: anywhere; }
.step-content > p:first-child { color: var(--text); }
.step-content > * + * { margin-top: 10px; }
.step-content :deep(.install-command code) { white-space: pre-wrap; overflow-wrap: anywhere; }
.guide-note { color: var(--muted); font-size: 14px; line-height: 1.6; }
.browser-note { margin-block-start: 12px; margin-inline-start: 42px; }
.guide-preview {
  height: 134px;
  padding: 16px clamp(6px, 2vw, 16px);
  overflow: hidden;
  border: 1px solid var(--edge);
  border-radius: 12px;
  background: #141416;
}
.guide-list { list-style: disc; margin: 0; padding-inline-start: 1.25em; }
.guide-list li + li { margin-top: 10px; }
.guide-list li::marker { color: #d78476; }
.guide-list strong { color: var(--text); font-weight: 600; }
@media (max-width: 720px) {
  .remote-page { padding-block: 64px; }
  .guide-layout { grid-template-columns: minmax(0, 1fr); margin-top: 32px; }
  .guide-directory { position: static; max-height: none; border-block: 1px solid var(--edge); }
  .guide-directory details { display: block; }
  .guide-directory summary { cursor: pointer; padding-block: 12px; }
  .guide-directory nav { display: none; padding-block-end: 10px; }
  .guide-directory details[open] + nav { display: block; }
  .guide-directory p { display: none; }
  .guide-section + .guide-section { margin-top: 32px; padding-top: 24px; }
}
</style>
