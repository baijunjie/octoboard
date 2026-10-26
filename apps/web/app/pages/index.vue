<script setup lang="ts">
import {
  developedPlatform,
  developerSections,
  heroAgents,
  homeQuestions,
  homeSteps,
  plannedPlatforms,
} from "../../seo/page-outline";
const { t } = useSiteCopy();
const config = useRuntimeConfig();
const localePath = useLocalePath();
const page = ref<HTMLElement | null>(null);
const paused = ref(false);
const releasesUrl = `${config.public.repositoryUrl}/releases`;
useSectionReveal(page);
useSeoMeta({
  title: () => t("workspaceSeoTitle"),
  description: () => t("workspaceSeoDescription"),
  ogTitle: () => t("workspaceSeoTitle"),
  ogDescription: () => t("workspaceSeoDescription"),
});
</script>

<template>
  <main
    id="main-content"
    ref="page"
    class="home-page"
    :class="{ 'motion-paused': paused }"
  >
    <section class="hero">
      <div class="hero-grid" aria-hidden="true" />
      <AmbientParticles :paused="paused" />
      <div class="hero-layout wrap">
        <div class="hero-copy">
          <p class="eyebrow">
            <span class="red-square" aria-hidden="true" />{{ t("brandSlogan") }}
          </p>
          <h1>
            <span>{{ t("workspaceTitle") }}</span
            ><span>{{ t("workspaceTitleSecond") }}</span
            ><span class="metal-type">{{ t("workspaceTitleAccent") }}</span>
          </h1>
          <p class="hero-description">{{ t("workspaceDescription") }}</p>
          <div class="hero-actions">
            <a :href="releasesUrl" class="button"
            ><SiteIcon name="download-simple" />{{ t("heroCta") }}</a
            >
            <a
              :href="config.public.repositoryUrl"
              class="github-link"
              :aria-label="t('heroSecondary')"
              :title="t('heroSecondary')"
            ><SiteIcon name="github-logo"
            /></a>
            <a href="#product-preview" class="text-link"
            >{{ t("previewEyebrow")
            }}<span class="link-arrow" aria-hidden="true"
            /></a>
          </div>
          <p class="hero-note">{{ t("heroNote") }}</p>
        </div>
        <div class="hero-art">
          <HeroMascot :paused="paused" />
          <button
            class="motion-toggle"
            type="button"
            :aria-label="t(paused ? 'motionPlay' : 'motionPause')"
            @click="paused = !paused"
          >
            <span
              :class="paused ? 'play-symbol' : 'pause-symbol'"
              aria-hidden="true"
            />
            {{ t(paused ? "motionPlay" : "motionPause") }}
          </button>
        </div>
      </div>
      <div class="hero-bottom wrap">
        <div class="agent-rail">
          <span class="eyebrow">{{ t("workspaceAgentLabel") }}</span>
          <div>
            <span v-for="agent in heroAgents" :key="agent.icon"
            ><img
              :src="`${config.app.baseURL}agents/${agent.icon}.svg`"
              alt=""
              width="24"
              height="24"
            />{{ agent.name }}</span
            >
          </div>
        </div>
        <div class="hero-end">
          <span class="hero-status">{{ t("heroStatus") }}</span
          ><a href="#benefits" class="explore-link"
          >{{ t("workspaceScroll") }}<span class="down-arrow" aria-hidden="true"
          /></a>
        </div>
      </div>
    </section>

    <section id="benefits" class="continuity-section wrap section-space" data-reveal>
      <SectionHeading
        number="01"
        :label="t('continuityEyebrow')"
        :title="t('continuityTitle')"
      />
      <ContinuityFeatures />
    </section>

    <section
      id="product-preview"
      class="product-section section-space"
      data-reveal
    >
      <div class="wrap">
        <SectionHeading
          number="02"
          :label="t('previewEyebrow')"
          :title="t('previewTitle')"
          :description="t('previewDescription')"
        /><ProductPreview />
      </div>
    </section>

    <section
      id="how-it-works"
      class="workflow-section wrap section-space"
      data-reveal
    >
      <SectionHeading
        number="03"
        :label="t('coordinationEyebrow')"
        :title="t('coordinationTitle')"
        :description="t('coordinationDescription')"
      />
      <WorkflowDemo :paused="paused" />
      <div class="workflow-steps">
        <article v-for="(step, index) in homeSteps" :key="step">
          <span class="step-number" aria-hidden="true">0{{ index + 1 }}</span>
          <h3>{{ t(`step${step}Title`) }}</h3>
          <p>{{ t(`step${step}Description`) }}</p>
        </article>
      </div>
    </section>

    <section id="developer-intent" class="developer-section wrap section-space">
      <SectionHeading
        number="04"
        :label="t('developerEyebrow')"
        :title="t('developerTitle')"
      />
      <div class="developer-letter">
        <p class="developer-intro">{{ t("developerIntro") }}</p>
        <div v-for="section in developerSections" :key="section" class="developer-point">
          <h3>{{ t(`developer${section}Title`) }}</h3>
          <p>{{ t(`developer${section}Body`) }}</p>
        </div>
        <p>{{ t("developerPurpose") }}</p>
        <p class="developer-invitation">{{ t("developerInvitation") }}</p>
        <p class="developer-byline">{{ t("developerByline") }}</p>
      </div>
    </section>

    <section id="local-workspace" class="trust-section wrap section-space" data-reveal>
      <SectionHeading
        number="05"
        :label="t('localEyebrow')"
        :title="t('localTitle')"
        :description="t('localDescription')"
      />
      <article class="open-source-panel">
        <span class="license-word" aria-hidden="true">MIT</span>
        <div>
          <h3 class="preserve-lines">{{ t("proofTitle") }}</h3>
          <p>{{ t("proofDescription") }}</p>
          <p>{{ t("proofFreeCaption") }}</p>
          <div class="trust-links">
            <a
              :href="config.public.repositoryUrl"
              class="github-link"
              :aria-label="t('proofRepository')"
              :title="t('proofRepository')"
            ><SiteIcon name="github-logo" /></a
            ><a
              :href="`${config.public.repositoryUrl}/blob/main/LICENSE`"
              class="text-link"
            >{{ t("proofLicense")
            }}<span class="link-arrow" aria-hidden="true"
            /></a>
            <NuxtLink :to="localePath('/privacy/')" class="text-link"
            >{{ t("localLink") }}<span class="link-arrow" aria-hidden="true"
            /></NuxtLink>
          </div>
        </div>
      </article>
      <div class="platform-intro">
        <h3>{{ t("platformsTitle") }}</h3>
        <p>{{ t("platformsDescription") }}</p>
      </div>
      <ul class="platform-list">
        <li class="platform-current">
          <SiteIcon :name="developedPlatform.icon" />
          <div>
            <h4>{{ developedPlatform.name }}</h4>
            <p>{{ t("platformsMacStatus") }}</p>
          </div>
        </li>
        <li v-for="platform in plannedPlatforms" :key="platform.name">
          <SiteIcon :name="platform.icon" />
          <div>
            <h4>{{ platform.name }}</h4>
            <p>{{ t(platform.status) }}</p>
          </div>
        </li>
      </ul>
    </section>

    <section id="questions" class="faq-section wrap section-space" data-reveal>
      <SectionHeading number="06" :label="t('faqEyebrow')" :title="t('faqTitle')" />
      <div class="faq-list">
        <details
          v-for="(question, index) in homeQuestions"
          :key="question"
          :open="index === 0"
        >
          <summary>
            <span class="question-number" aria-hidden="true"
            >0{{ index + 1 }}</span
            >
            <h3>{{ t(`faq${question}Question`) }}</h3>
            <span class="faq-toggle" aria-hidden="true" />
          </summary>
          <p>{{ t(`faq${question}Answer`) }}</p>
        </details>
      </div>
    </section>

    <section class="final-cta">
      <div class="wrap final-cta-content" data-reveal>
        <div class="cta-brand">
          <img
            class="cta-brand-mascot"
            :src="`${config.app.baseURL}logo-640.png`"
            alt=""
            width="640"
            height="640"
            loading="lazy"
          />
          <div class="cta-brand-name">
            <img
              :src="`${config.app.baseURL}wordmark.png`"
              :alt="config.public.appName"
              width="2044"
              height="344"
              loading="lazy"
            />
            <p class="cta-brand-slogan">{{ t("brandSlogan") }}</p>
          </div>
        </div>
        <p class="cta-brand-meaning">{{ t("brandSloganMeaning") }}</p>
        <a :href="releasesUrl" class="button"
        ><SiteIcon name="download-simple" />{{ t("heroCta") }}</a
        >
        <p class="cta-note">{{ t("ctaNote") }}</p>
      </div>
    </section>
  </main>
</template>

<style scoped>
.developer-letter {
  max-width: 48rem;
  margin-inline: auto;
  display: grid;
  gap: 32px;
  color: var(--muted);
  font-size: 17px;
  line-height: 1.85;
  overflow-wrap: anywhere;
}
.developer-intro,
.developer-invitation {
  color: var(--text);
  font-size: clamp(20px, 2vw, 24px);
  line-height: 1.65;
}
.developer-point h3 {
  margin-bottom: 12px;
  color: var(--text);
  font-size: 20px;
  line-height: 1.5;
}
.developer-invitation {
  border-inline-start: 2px solid var(--red);
  padding-inline-start: 24px;
}
.developer-byline {
  font-size: 14px;
}
</style>
