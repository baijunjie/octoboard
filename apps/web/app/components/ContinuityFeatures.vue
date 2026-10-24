<script setup lang="ts">
import { heroAgents, homeFeatures } from "../../seo/page-outline";

const { t } = useSiteCopy();
const { locale } = useI18n();
const config = useRuntimeConfig();
const percentage = (value: number) =>
  new Intl.NumberFormat(locale.value, { style: "percent" }).format(value);
</script>

<template>
  <div class="continuity-features">
    <article v-for="feature in homeFeatures" :key="feature.key" class="feature-row">
      <div class="feature-copy">
        <p class="feature-label">{{ t(`${feature.key}FeatureLabel`) }}</p>
        <h3 class="preserve-lines">{{ t(`${feature.key}FeatureTitle`) }}</h3>
        <p class="feature-description">{{ t(`${feature.key}FeatureDescription`) }}</p>
      </div>

      <div class="feature-visual" aria-hidden="true">
        <template v-if="feature.key === 'account'">
          <div class="account-pair">
            <div class="account-card">
              <div class="account-identity">
                <img :src="`${config.app.baseURL}agents/codex.svg`" alt="" width="28" height="28" />
                <span>Codex <bdi>A</bdi></span>
              </div>
              <span class="account-meter"><span style="width: 88%" /></span>
              <bdi>{{ percentage(0.88) }}</bdi>
            </div>
            <SiteIcon name="arrow-right" class="direction-arrow" />
            <div class="account-card account-destination">
              <div class="account-identity">
                <img :src="`${config.app.baseURL}agents/codex.svg`" alt="" width="28" height="28" />
                <span>Codex <bdi>B</bdi></span>
              </div>
              <span class="account-meter"><span style="width: 24%" /></span>
              <bdi>{{ percentage(0.24) }}</bdi>
            </div>
          </div>
          <div class="context-thread">
            <SiteIcon name="chat-circle-dots" />
            <span>{{ t("accountExampleContext") }}</span>
            <span class="thread-line" />
          </div>
        </template>

        <template v-else-if="feature.key === 'handoff'">
          <div class="handoff-agents">
            <template v-for="(agent, index) in heroAgents" :key="agent.icon">
              <SiteIcon v-if="index" name="arrow-right" class="direction-arrow" />
              <div class="handoff-agent">
                <span class="agent-orbit">
                  <img :src="`${config.app.baseURL}agents/${agent.icon}.svg`" alt="" width="36" height="36" />
                </span>
                <span>{{ agent.name }}</span>
              </div>
            </template>
          </div>
          <div class="handoff-brief">
            <span class="brief-rule" />
            <span>{{ t("handoffExampleBrief") }}</span>
            <span class="brief-rule" />
          </div>
        </template>

        <template v-else>
          <div class="usage-dashboard">
            <div class="usage-ring"><bdi>{{ percentage(0.68) }}</bdi></div>
            <div class="usage-clock">
              <p>{{ t("usageExampleWindow") }}</p>
              <bdi class="clock-digits" dir="ltr">02:14:08</bdi>
              <p class="reset-label"><span class="clock-dot" />{{ t("usageExampleReset") }}</p>
            </div>
          </div>
        </template>
      </div>
    </article>
    <p class="illustration-caption">{{ t("continuityIllustration") }}</p>
  </div>
</template>

<style scoped>
.feature-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  align-items: center;
  gap: 80px;
  padding-block: 48px;
  border-top: 1px solid var(--edge);
}
.feature-label {
  margin-bottom: 20px;
  font-size: 13px;
  color: var(--red);
}
.feature-copy h3 {
  font-size: clamp(26px, 3vw, 38px);
  font-weight: 500;
  line-height: 1.3;
  letter-spacing: -0.03em;
}
.feature-description {
  max-width: 480px;
  margin-top: 24px;
  color: var(--muted);
  font-size: 16px;
  line-height: 1.85;
}
.feature-visual {
  min-width: 0;
  padding: 40px 32px;
  border-radius: 16px;
  background: radial-gradient(ellipse at 50% 100%, #30201c55, transparent 75%), #111;
  border: 1px solid #242424;
}
.account-pair {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 16px;
}
.account-card {
  padding: 20px;
  background: #1a1a1a;
  border: 1px solid #363636;
  border-radius: 10px;
  font-size: 13px;
  color: #b8b8b8;
}
.account-identity {
  display: flex;
  align-items: center;
  gap: 10px;
  color: var(--text);
}
.account-identity span {
  white-space: nowrap;
}
.account-meter,
.account-meter span {
  display: block;
  height: 4px;
  border-radius: 2px;
}
.account-meter {
  margin-block: 24px 10px;
  background: #373737;
}
.account-meter span {
  background: #aaa;
}
.account-destination .account-meter span {
  background: var(--red);
}
.direction-arrow {
  color: #b8b8b8;
  width: 20px;
  height: 20px;
}
.feature-visual:dir(rtl) .direction-arrow {
  transform: scaleX(-1);
}
.context-thread {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 24px;
  color: #b8b8b8;
  font-size: 13px;
}
.thread-line {
  flex: 1;
  height: 1px;
  background: #49403d;
}
.handoff-agents {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: start;
  gap: 12px;
}
.handoff-agents .direction-arrow {
  margin-top: 28px;
}
.handoff-agent {
  display: flex;
  align-items: center;
  flex-direction: column;
  gap: 16px;
  text-align: center;
  font-size: 12px;
  color: #c4c4c4;
}
.agent-orbit {
  display: grid;
  place-items: center;
  width: 76px;
  height: 76px;
  border: 1px solid #45403d;
  border-radius: 50%;
  background: #1b1918;
}
.handoff-brief {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-top: 32px;
  color: #b8b8b8;
  text-align: center;
  font-size: 13px;
}
.brief-rule {
  flex: 1;
  min-width: 12px;
  height: 1px;
  background: #49403d;
}
.usage-dashboard {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 40px;
  padding-block: 12px;
}
.usage-ring {
  display: grid;
  place-items: center;
  position: relative;
  flex-shrink: 0;
  width: 132px;
  height: 132px;
  border-radius: 50%;
  background: conic-gradient(var(--red) 68%, #333 68%);
}
.usage-ring::before {
  content: "";
  position: absolute;
  inset: 6px;
  border-radius: 50%;
  background: #151313;
}
.usage-ring bdi {
  position: relative;
  font-size: 30px;
  letter-spacing: -0.04em;
}
.usage-clock {
  min-width: 0;
  color: #b8b8b8;
  font-size: 13px;
}
.clock-digits {
  display: block;
  margin-block: 12px;
  color: var(--text);
  font-size: clamp(24px, 2.8vw, 36px);
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.03em;
}
.reset-label {
  display: flex;
  align-items: center;
  gap: 8px;
}
.clock-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--red);
  flex-shrink: 0;
}
.illustration-caption {
  text-align: end;
  color: var(--muted);
  font-size: 12px;
}
@media (max-width: 1100px) {
  .feature-row {
    gap: 40px;
  }
  .feature-visual {
    padding: 32px 20px;
  }
  .account-card {
    padding: 16px 12px;
  }
  .usage-dashboard {
    gap: 24px;
  }
}
@media (max-width: 800px) {
  .feature-row {
    grid-template-columns: minmax(0, 1fr);
    gap: 28px;
    padding-block: 32px;
  }
  .feature-description {
    max-width: none;
    margin-top: 16px;
  }
  .feature-visual {
    width: 100%;
    max-width: 560px;
    justify-self: center;
  }
}
@media (max-width: 380px) {
  .feature-visual {
    padding-inline: 12px;
  }
  .account-pair,
  .handoff-agents {
    gap: 8px;
  }
  .account-identity {
    gap: 6px;
  }
  .account-identity img {
    width: 20px;
    height: 20px;
  }
  .agent-orbit {
    width: 60px;
    height: 60px;
  }
  .handoff-agents .direction-arrow {
    margin-top: 20px;
  }
  .usage-ring {
    width: 108px;
    height: 108px;
  }
}
</style>
