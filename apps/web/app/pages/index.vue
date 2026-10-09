<script setup lang="ts">
import {
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
  <main id="main-content" ref="page">
    <section class="hero section-frame">
      <AmbientParticles />
      <div class="hero-layout wrap">
        <div class="hero-copy">
          <p class="eyebrow">
            <span class="red-square" />{{ t("brandSlogan") }}
          </p>
          <h1>
            <span>{{ t("heroTitle") }}</span
            ><span>{{ t("heroTitleSecond") }}</span
            ><span class="metal-type">{{ t("heroTitleAccent") }}</span>
          </h1>
          <p class="hero-description">{{ t("heroDescription") }}</p>
          <a :href="releasesUrl" class="button hero-button"
            ><SiteIcon name="download-simple" />{{ t("heroCta") }}</a
          >
          <p class="hero-status">{{ t("heroStatus") }}</p>
          <p class="hero-note">{{ t("heroNote") }}</p>
        </div>
        <div class="hero-art">
          <div class="target-bracket bracket-tl" aria-hidden="true" />
          <div class="target-bracket bracket-br" aria-hidden="true" />
          <img
            class="hero-mascot"
            :src="`${config.app.baseURL}logo.png`"
            :alt="t('heroAssetAlt')"
            width="1254"
            height="1254"
            fetchpriority="high"
          />
        </div>
      </div>
      <div class="hero-bottom wrap">
        <div class="agent-rail">
          <span class="eyebrow">{{ t("heroAgentLabel") }}</span>
          <div>
            <span v-for="agent in heroAgents" :key="agent.icon">
              <img
                :src="`${config.app.baseURL}agents/${agent.icon}.svg`"
                alt=""
                width="24"
                height="24"
              />
              {{ agent.name }}
            </span>
          </div>
        </div>
        <NuxtLink :to="`${localePath('/')}#benefits`" class="explore-link"
          >{{ t("heroScroll") }}<span class="down-arrow" aria-hidden="true"
        /></NuxtLink>
      </div>
    </section>

    <section
      id="benefits"
      class="benefits-section wrap section-space"
      data-reveal
    >
      <div class="section-topline">
        <span class="section-index">{{ t("benefitsIndex") }}</span
        ><span class="crosshair" aria-hidden="true" />
      </div>
      <div class="benefits-layout">
        <div class="benefits-heading">
          <p class="eyebrow">{{ t("principlesEyebrow") }}</p>
          <h2 class="preserve-lines">{{ t("principlesTitle") }}</h2>
          <div class="pixel-stack" aria-hidden="true">
            <span /><span /><span /><span /><span /><span /><span />
          </div>
        </div>
        <div class="benefits-list">
          <article v-for="feature in homeFeatures" :key="feature.key">
            <SiteIcon :name="feature.icon" class="benefit-icon" />
            <div>
              <h3>{{ t(`feature${feature.key}Title`) }}</h3>
              <p>{{ t(`feature${feature.key}Description`) }}</p>
            </div>
          </article>
        </div>
      </div>
    </section>

    <section class="tagline-section section-frame">
      <div class="wrap tagline-layout" data-reveal>
        <WordReveal :text="t('revealTagline')" />
        <div class="collaboration-emblem" aria-hidden="true">
          <svg class="emblem-circuit" viewBox="0 0 420 300" fill="none">
            <path
              class="emblem-track"
              d="M72 50H152L232 150H300M72 150H300M72 250H152L232 150"
            />
            <path class="emblem-signal" d="M72 150H300" />
          </svg>
          <div class="emblem-agent emblem-claude">
            <img
              :src="`${config.app.baseURL}agents/claude.svg`"
              alt=""
              width="40"
              height="40"
              loading="lazy"
            />
          </div>
          <div class="emblem-agent emblem-codex">
            <img
              :src="`${config.app.baseURL}agents/codex.svg`"
              alt=""
              width="40"
              height="40"
              loading="lazy"
            />
          </div>
          <div class="emblem-agent emblem-grok">
            <img
              :src="`${config.app.baseURL}agents/grok.svg`"
              alt=""
              width="40"
              height="40"
              loading="lazy"
            />
          </div>
          <div class="emblem-core">
            <img
              :src="`${config.app.baseURL}logo-640.png`"
              alt=""
              width="160"
              height="160"
              loading="lazy"
            />
          </div>
        </div>
      </div>
    </section>

    <section
      id="how-it-works"
      class="workflow-section wrap section-space"
      data-reveal
    >
      <div class="section-topline">
        <span class="section-index">{{ t("workflowIndex") }}</span
        ><span class="crosshair" aria-hidden="true" />
      </div>
      <div class="section-heading">
        <div>
          <p class="eyebrow">{{ t("howEyebrow") }}</p>
          <h2 class="preserve-lines">{{ t("howTitle") }}</h2>
        </div>
        <p>{{ t("howDescription") }}</p>
      </div>
      <WorkflowDemo />
      <div class="workflow-steps">
        <article v-for="(step, index) in homeSteps" :key="step">
          <span class="step-number">0{{ index + 1 }}</span>
          <h3>{{ t(`step${step}Title`) }}</h3>
          <p>{{ t(`step${step}Description`) }}</p>
        </article>
      </div>
    </section>

    <section class="proof-section section-frame">
      <div class="wrap section-space" data-reveal>
        <div class="section-topline">
          <span class="section-index">{{ t("proofIndex") }}</span
          ><span class="crosshair" aria-hidden="true" />
        </div>
        <div class="proof-layout">
          <div>
            <h2 class="preserve-lines">{{ t("proofTitle") }}</h2>
            <p>{{ t("proofDescription") }}</p>
            <a :href="config.public.repositoryUrl" class="text-link"
              >{{ t("proofRepository")
              }}<span class="link-arrow" aria-hidden="true"
            /></a>
          </div>
          <div class="license-panel">
            <span class="license-word">{{ t("proofBadge") }}</span>
            <div class="license-panel-bottom">
              <p>{{ t("proofCaption") }}</p>
              <a
                :href="`${config.public.repositoryUrl}/blob/main/LICENSE`"
                class="text-link"
                >{{ t("proofLicense")
                }}<span class="link-arrow" aria-hidden="true"
              /></a>
            </div>
            <span class="panel-pin pin-one" aria-hidden="true" /><span
              class="panel-pin pin-two"
              aria-hidden="true"
            />
          </div>
        </div>
      </div>
    </section>

    <section class="data-section wrap section-space" data-reveal>
      <div class="section-topline">
        <span class="section-index">{{ t("privacyIndex") }}</span
        ><span class="crosshair" aria-hidden="true" />
      </div>
      <div class="data-layout">
        <div class="local-main">
          <p class="eyebrow">
            <span class="status-square" />{{ t("localEyebrow") }}
          </p>
          <h2>{{ t("localTitle") }}</h2>
          <p>{{ t("localDescription") }}</p>
          <NuxtLink :to="localePath('/privacy/')" class="text-link"
            >{{ t("localLink") }}<span class="link-arrow" aria-hidden="true"
          /></NuxtLink>
        </div>
        <div class="remote-main">
          <span class="planned-badge">{{ t("remoteBadge") }}</span>
          <div class="remote-illustration" aria-hidden="true">
            <div class="remote-device remote-device-local">
              <div class="remote-screen">
                <SiteIcon name="terminal-window" />
              </div>
              <div class="remote-keyboard"><i /><i /><i /><i /><i /></div>
            </div>
            <div class="remote-signal">
              <i /><i /><SiteIcon name="plugs-connected" /><i /><i />
            </div>
            <div class="remote-device remote-device-host">
              <img
                :src="`${config.app.baseURL}logo-640.png`"
                alt=""
                width="96"
                height="96"
                loading="lazy"
              />
              <div class="remote-vents"><i /><i /><i /><i /></div>
            </div>
            <div class="remote-floor"><i /><i /><i /><i /><i /></div>
          </div>
          <h3>{{ t("remoteTitle") }}</h3>
          <p>{{ t("remoteDescription") }}</p>
        </div>
      </div>
    </section>

    <section
      class="platform-section wrap"
      aria-labelledby="platforms-title"
      data-reveal
    >
      <h2 id="platforms-title">{{ t("platformsTitle") }}</h2>
      <p>{{ t("platformsDescription") }}</p>
      <ul class="platform-list">
        <li class="platform-current">
          <div class="platform-emblem">
            <SiteIcon :name="developedPlatform.icon" />
          </div>
          <div class="platform-copy">
            <h3>{{ developedPlatform.name }}</h3>
            <p>{{ t("platformsMacStatus") }}</p>
          </div>
        </li>
        <li v-for="platform in plannedPlatforms" :key="platform.name">
          <div class="platform-emblem"><SiteIcon :name="platform.icon" /></div>
          <div class="platform-copy">
            <h3>{{ platform.name }}</h3>
            <p>{{ t("remoteBadge") }}</p>
          </div>
        </li>
      </ul>
    </section>

    <section id="questions" class="faq-section wrap section-space" data-reveal>
      <div class="section-topline">
        <span class="section-index">{{ t("faqIndex") }}</span
        ><span class="crosshair" aria-hidden="true" />
      </div>
      <h2>{{ t("faqTitle") }}</h2>
      <div class="faq-list">
        <details
          v-for="question in homeQuestions"
          :key="question"
          :open="question === 'One'"
        >
          <summary>
            <h3>{{ t(`faq${question}Question`) }}</h3>
            <SiteIcon name="plus" class="faq-expand" />
            <SiteIcon name="minus" class="faq-collapse" />
          </summary>
          <p>{{ t(`faq${question}Answer`) }}</p>
        </details>
      </div>
    </section>

    <section class="final-cta section-frame">
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
        <p class="eyebrow">{{ t("ctaEyebrow") }}</p>
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
