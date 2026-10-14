<script setup lang="ts">
import {
  collaborationScenarios,
  developedPlatform,
  heroAgents,
  homeFeatures,
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
  title: () => t("seoTitle"),
  description: () => t("seoDescription"),
  ogTitle: () => t("seoTitle"),
  ogDescription: () => t("seoDescription"),
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
            <span>{{ t("heroTitle") }}</span
            ><span>{{ t("heroTitleSecond") }}</span
            ><span class="metal-type">{{ t("heroTitleAccent") }}</span>
          </h1>
          <p class="hero-description">{{ t("heroDescription") }}</p>
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
          <span class="eyebrow">{{ t("heroAgentLabel") }}</span>
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
            >{{ t("heroScroll") }}<span class="down-arrow" aria-hidden="true"
          /></a>
        </div>
      </div>
    </section>

    <section id="benefits" class="pain-section wrap section-space" data-reveal>
      <SectionHeading
        number="01"
        :label="t('painEyebrow')"
        :title="t('painTitle')"
        :description="t('painDescription')"
      />
      <div class="pain-equation">
        <article class="pain-pillar">
          <div class="pillar-symbol" aria-hidden="true">
            <span /><span /><span /><span /><span /><span />
          </div>
          <h3>{{ t("painManagementTitle") }}</h3>
          <p>{{ t("painManagementDescription") }}</p>
        </article>
        <span class="equation-plus" aria-hidden="true">+</span>
        <article class="pain-pillar">
          <div class="quality-symbol" aria-hidden="true">
            <span />
          </div>
          <h3>{{ t("painQualityTitle") }}</h3>
          <p>{{ t("painQualityDescription") }}</p>
        </article>
      </div>
      <p class="pain-resolution">
        <span class="red-square" aria-hidden="true" />{{ t("painResolution") }}
      </p>
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
        :label="t('howEyebrow')"
        :title="t('howTitle')"
        :description="t('howDescription')"
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

    <section class="principles-section section-space" data-reveal>
      <div class="wrap">
        <SectionHeading
          number="04"
          :label="t('principlesEyebrow')"
          :title="t('principlesTitle')"
        />
        <div class="principle-grid">
          <article v-for="(feature, index) in homeFeatures" :key="feature.key">
            <div class="principle-top">
              <SiteIcon :name="feature.icon" /><span aria-hidden="true"
                >0{{ index + 1 }}</span
              >
            </div>
            <h3>{{ t(`feature${feature.key}Title`) }}</h3>
            <p>{{ t(`feature${feature.key}Description`) }}</p>
            <ul v-if="feature.key === 'Three'" class="collaboration-scenarios">
              <li v-for="scenario in collaborationScenarios" :key="scenario">
                <h4>{{ t(`featureThree${scenario}Title`) }}</h4>
                <p>{{ t(`featureThree${scenario}Description`) }}</p>
              </li>
            </ul>
          </article>
        </div>
      </div>
    </section>

    <section class="trust-section wrap section-space" data-reveal>
      <SectionHeading
        number="05"
        :label="t('localEyebrow')"
        :title="t('localTitle')"
        :description="t('localDescription')"
      />
      <div class="trust-grid">
        <article class="open-source-panel">
          <span class="license-word" aria-hidden="true">MIT</span>
          <div>
            <h3 class="preserve-lines">{{ t("proofTitle") }}</h3>
            <p>{{ t("proofDescription") }}</p>
            <p>{{ t("proofCaption") }}</p>
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
            </div>
          </div>
        </article>
        <article class="remote-panel">
          <span class="planned-badge">{{ t("remoteBadge") }}</span>
          <h3>{{ t("remoteTitle") }}</h3>
          <p>{{ t("remoteDescription") }}</p>
          <NuxtLink :to="localePath('/privacy/')" class="text-link"
            >{{ t("localLink") }}<span class="link-arrow" aria-hidden="true"
          /></NuxtLink>
        </article>
      </div>
    </section>

    <section class="platform-section wrap section-space" data-reveal>
      <SectionHeading
        number="06"
        :label="t('heroStatus')"
        :title="t('platformsTitle')"
        :description="t('platformsDescription')"
      />
      <ul class="platform-list">
        <li class="platform-current">
          <SiteIcon :name="developedPlatform.icon" />
          <div>
            <h3>{{ developedPlatform.name }}</h3>
            <p>{{ t("platformsMacStatus") }}</p>
          </div>
        </li>
        <li v-for="platform in plannedPlatforms" :key="platform.name">
          <SiteIcon :name="platform.icon" />
          <div>
            <h3>{{ platform.name }}</h3>
            <p>{{ t("remoteBadge") }}</p>
          </div>
        </li>
      </ul>
    </section>

    <section id="questions" class="faq-section wrap section-space" data-reveal>
      <SectionHeading number="07" :label="t('navFaq')" :title="t('faqTitle')" />
      <div class="faq-list">
        <details
          v-for="(question, index) in homeQuestions"
          :key="question"
          :open="question === 'One'"
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
        <p class="eyebrow">
          <span class="red-square" aria-hidden="true" />{{ t("ctaEyebrow") }}
        </p>
        <h2 class="preserve-lines">{{ t("ctaTitle") }}</h2>
        <p class="cta-description">{{ t("ctaDescription") }}</p>
        <a :href="releasesUrl" class="button"
          ><SiteIcon name="download-simple" />{{ t("heroCta") }}</a
        >
        <p class="cta-note">{{ t("ctaNote") }}</p>
      </div>
    </section>
  </main>
</template>
