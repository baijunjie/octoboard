<script setup lang="ts">
defineProps<{ feature: string }>();
const { t } = useSiteCopy();
const { locale } = useI18n();
const appViews = ["managedAccounts", "usageMonitor", "startUsageWindows", "keepAwake", "sessionIntervention", "scheduledTasks", "nativeProjectSessions", "reviewChanges", "processMonitoring", "openAgentApp", "sharedMemory"];
const sharingTools = [["code", "featureVisualSkills"], ["coordinator", "featureVisualPlugins"], ["branch", "featureVisualMcp"]] as const;
const ids = ["A", "B", "C"];
const providers = ["Claude Code", "Codex", "Grok Build"];
const agents = ["claude", "codex", "grok"] as const;
const account = (id: string) => t("featureVisualAccount", { id: `\u2068${id}\u2069` });
const percent = (value: number) => new Intl.NumberFormat(locale.value, { style: "percent", maximumFractionDigits: 0 }).format(value / 100);
</script>

<template>
  <div class="feature-illustration" :class="[`illustration-${feature}`, { 'has-app-view': appViews.includes(feature) }]" aria-hidden="true">
    <FeatureAppView v-if="appViews.includes(feature)" :feature="feature" />

    <div v-else-if="feature === 'accountContinuity'" class="switch-scene">
      <div class="provider-heading"><AgentMark agent="claude" /><b>Claude Code</b><span class="chip">{{ t('featureVisualAuto') }}</span></div>
      <div class="switch-track">
        <div class="account-tile visual-node"><FeatureGlyph name="account" /><b>{{ account('A') }}</b><span class="limit-label">{{ t('featureVisualLimit') }}</span></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="account-tile visual-node is-accent"><FeatureGlyph name="account" /><b>{{ account('B') }}</b></div>
      </div>
      <div class="retained-session"><FeatureGlyph name="chat" :size="17" /><span>{{ t('accountExampleContext') }}</span><FeatureGlyph name="check" :size="14" /></div>
    </div>

    <div v-else-if="feature === 'sharedConfiguration'" class="sharing-scene">
      <div class="provider-heading"><AgentMark agent="codex" /><b>Codex</b></div>
      <div class="sharing-accounts">
        <div v-for="id in ids.slice(0, 2)" :key="id" class="sharing-account">
          <FeatureGlyph name="account" :size="24" /><b>{{ account(id) }}</b>
        </div>
      </div>
      <div class="sharing-join" />
      <div class="sharing-tools visual-surface">
        <div v-for="[icon, key] in sharingTools" :key="key">
          <FeatureGlyph :name="icon" :size="23" /><b>{{ t(key) }}</b>
        </div>
      </div>
    </div>

    <div v-else-if="feature === 'quotaScheduling'" class="recovery-scene">
      <div class="recovery-stages">
        <div class="recovery-stage"><FeatureGlyph name="pause-circle" class="stage-symbol" :size="36" /><span>{{ t('featureVisualWaiting') }}</span></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="recovery-stage"><FeatureGlyph name="check-circle" class="stage-symbol" :size="36" /><span>{{ t('featureVisualCheckQuota') }}</span></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="recovery-stage is-current"><FeatureGlyph name="play-circle" class="stage-symbol" :size="36" /><span>{{ t('featureVisualResume') }}</span></div>
      </div>
      <div class="recovery-task visual-surface"><div class="task-bar"><FeatureGlyph name="code" :size="16" /><span>{{ t('featureVisualTask') }}</span><FeatureGlyph name="working" class="working-mark" :size="10" /></div><div class="terminal-snippet"><FeatureGlyph name="prompt" :size="13" /><code>pnpm test</code><span class="terminal-cursor" /></div></div>
    </div>

    <div v-else-if="feature === 'freshSession'" class="relay-scene">
      <div class="relay-sessions">
        <div class="session-sheet visual-node"><div class="mini-heading"><FeatureGlyph name="chat" :size="14" /><b>{{ t('featureVisualEarlier') }}</b></div><div class="session-context-usage"><span>{{ t('featureVisualContext') }}</span><bdi>{{ percent(82) }}</bdi><span class="small-meter"><i style="width: 82%" /></span></div></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="session-sheet visual-node is-accent"><div class="mini-heading"><FeatureGlyph name="chat" :size="14" /><b>{{ t('featureVisualNext') }}</b></div><FeatureGlyph name="file" :size="21" /></div>
      </div>
      <div class="relay-bottom"><span class="history-branch"><FeatureGlyph name="history" :size="15" />{{ t('featureVisualHistory') }}</span><span class="relay-brief"><FeatureGlyph name="check" :size="13" />{{ t('featureVisualContinue') }}</span></div>
    </div>

    <div v-else-if="feature === 'agentHandoff'" class="handoff-scene">
      <div class="handoff-agents"><div class="agent-card visual-node"><AgentMark agent="claude" /><b>Claude Code</b></div><div class="handoff-packet"><FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" /><span class="packet-sheet"><FeatureGlyph name="file" :size="25" /></span><FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" /></div><div class="agent-card visual-node is-accent"><AgentMark agent="codex" /><b>Codex</b></div></div>
      <p class="handoff-caption">{{ t('handoffExampleBrief') }}</p>
    </div>

    <div v-else-if="feature === 'crossProjectWork'" class="network-scene">
      <div class="network-head">
        <div class="network-root visual-node is-accent">
          <FeatureGlyph name="coordinator" :size="17" /><b>{{ t('demoCoordinator') }}</b>
        </div>
      </div>
      <div class="network-trunk" /><div class="network-branches" />
      <div class="network-leaves"><div v-for="(provider, index) in providers" :key="provider" class="network-leaf visual-node"><AgentMark :agent="agents[index]!" /><b>{{ provider }}</b><code>{{ ['web/', 'api/', 'docs/'][index] }}</code></div></div>
    </div>

  </div>
</template>

<style scoped>
.feature-illustration { --visual-accent: #d78476; position: relative; display: grid; align-items: center; min-width: 0; height: 272px; padding: 24px; overflow: hidden; border-bottom: 1px solid var(--edge); background: radial-gradient(ellipse at 50% 90%, #30201c33, transparent 75%), #111112; color: #b9b4bb; font-size: 12px; line-height: 1.4; user-select: none; }
.feature-illustration::before { content: ''; position: absolute; inset: 0; pointer-events: none; background-image: radial-gradient(#ffffff12 .5px, transparent .5px); background-size: 12px 12px; mask-image: radial-gradient(ellipse, #0005, transparent 80%); }
.feature-illustration > * { position: relative; }
.has-app-view { background: #141416; padding: 16px; }
.feature-illustration b { color: #e1dce1; font-weight: 500; font-size: 12px; overflow-wrap: anywhere; }
.feature-illustration code, .feature-illustration bdi { font-size: 11px; font-variant-numeric: tabular-nums; }
.feature-illustration code { direction: ltr; unicode-bidi: isolate; }
.feature-illustration svg { flex-shrink: 0; }
.visual-surface, .visual-node { min-width: 0; border: 1px solid #3a363b; background: linear-gradient(135deg, #282529, #1e1c20); border-radius: 9px; box-shadow: inset 0 1px #ffffff05, 0 6px 18px #0002; }
.is-accent { border-color: #99594e; background: linear-gradient(135deg, #372a29, #241e20); box-shadow: inset 0 1px #ffffff08, 0 0 0 3px #c77d6709, 0 6px 20px #0003; }
.small-meter { display: block; height: 3px; overflow: hidden; border-radius: 4px; background: #484047; }
.small-meter i { display: block; height: 100%; border-radius: inherit; background: #bd8276; }
.provider-heading, .mini-heading { display: flex; align-items: center; gap: 8px; min-width: 0; }
.provider-heading .chip { margin-inline-start: auto; }
.chip { padding: 3px 7px; border: 1px solid #65483f; border-radius: 5px; color: #cba094; font-size: 11px; }
.sharing-scene { display: grid; }
.sharing-scene .provider-heading { margin-bottom: 17px; }
.sharing-scene b { font-size: 14px; white-space: nowrap; }
.sharing-accounts, .sharing-tools { display: grid; text-align: center; }
.sharing-accounts { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
.sharing-account { display: flex; align-items: center; justify-content: center; gap: 8px; min-height: 35px; }
.sharing-join { height: 27px; position: relative; }
.sharing-join::before { content: ""; position: absolute; inset: 0 24% 50%; border: 1px solid #79554e; border-top: 0; border-radius: 0 0 5px 5px; }
.sharing-join::after { content: ""; position: absolute; inset: 50% 50% 0 auto; border-inline-start: 1px solid #79554e; }
.sharing-tools { grid-template-columns: repeat(3, minmax(0, 1fr)); padding: 14px 4px; }
.sharing-tools > div { display: grid; justify-items: center; align-content: start; gap: 7px; padding-inline: 3px; }
.sharing-tools svg { color: var(--visual-accent); }
.switch-scene { display: grid; gap: 15px; }
.switch-track { display: grid; grid-template-columns: minmax(0, 1fr) 28px minmax(0, 1fr); align-items: center; gap: 10px; }
.account-tile { position: relative; display: grid; justify-items: center; align-content: center; gap: 10px; padding: 14px 6px; min-height: 94px; text-align: center; }
.account-tile > svg { color: #bdabb4; }
.limit-label { color: #c4a39f; font-size: 11px; }
.retained-session { display: flex; align-items: center; gap: 8px; padding: 10px; border: 1px solid #51413e; border-radius: 7px; background: #221e21; font-size: 11px; }
.retained-session > span:nth-child(2) { flex: 1; }
.retained-session > svg:last-child { color: #caa08b; }
.recovery-scene { display: grid; gap: 26px; }
.recovery-stages { display: grid; grid-template-columns: auto 14px auto 14px auto; align-items: start; gap: 5px; }
.recovery-stages > .flow-arrow { margin-top: 8px; }
.recovery-stage { display: grid; white-space: nowrap; justify-items: center; gap: 10px; min-width: 0; text-align: center; font-size: 11px; overflow-wrap: anywhere; }
.recovery-task { padding: 11px 13px; }
.task-bar { display: flex; align-items: center; gap: 8px; font-size: 11px; color: #bfaeb6; }
.task-bar .working-mark { margin-inline-start: auto; }
.terminal-snippet { display: flex; align-items: center; gap: 9px; margin-top: 12px; direction: ltr; color: #cab8ad; }
.terminal-snippet > svg { color: #92828d; }
.terminal-cursor { display: block; width: 5px; height: 12px; background: #b99683; }
.session-context-usage { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 4px 8px; font-size: 11px; }
.session-context-usage .small-meter { grid-column: 1 / -1; }
.relay-scene { display: grid; gap: 19px; }
.relay-sessions { display: grid; grid-template-columns: minmax(0, 1fr) 20px minmax(0, 1fr); align-items: center; gap: 8px; }
.session-sheet { display: grid; align-content: start; gap: 10px; min-height: 83px; padding: 11px 9px; }
.mini-heading { padding-bottom: 8px; border-bottom: 1px solid #ffffff0c; }
.mini-heading b { font-size: 11px; white-space: nowrap; }
.relay-bottom { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 35px; align-items: center; font-size: 11px; }
.history-branch { display: flex; align-items: center; gap: 6px; position: relative; border: 1px dashed #62505b; border-radius: 5px; padding: 8px; }
.history-branch::before { content: ''; position: absolute; width: 1px; height: 19px; top: -20px; inset-inline-start: 20px; background: #62505b; }
.relay-brief { display: flex; align-items: center; gap: 6px; color: #c3a18e; }
.handoff-caption { text-align: center; font-size: 11px; }
.handoff-scene { display: grid; gap: 24px; }
.handoff-agents { display: grid; grid-template-columns: minmax(0, 1fr) 63px minmax(0, 1fr); align-items: center; gap: 8px; }
.agent-card { display: grid; justify-items: center; gap: 12px; padding: 17px 5px 13px; text-align: center; }
.agent-card .agent-mark { width: 28px; height: 28px; }
.handoff-packet { display: flex; align-items: center; gap: 3px; }
.handoff-packet .flow-arrow { flex: 1; min-width: 8px; }
.packet-sheet { display: grid; place-items: center; width: 31px; height: 39px; border: 1px solid #a77a62; background: #3a2d27; border-radius: 5px; color: #d6af91; transform: rotate(5deg); flex-shrink: 0; box-shadow: 3px 3px #19171c; }
.network-head { display: flex; justify-content: center; }
.network-root { display: flex; align-items: center; justify-content: center; gap: 6px; padding: 10px 14px; text-align: center; }
.network-root b { font-size: 11px; }
.network-trunk { width: 1px; height: 17px; margin-inline: auto; background: #946d5f; }
.network-branches { height: 12px; width: 69%; margin-inline: auto; border: 1px solid #806153; border-bottom: 0; border-radius: 5px 5px 0 0; position: relative; }
.network-branches::after { content: ''; position: absolute; top: 0; left: 50%; width: 1px; height: 12px; background: #806153; }
.network-leaves { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 7px; }
.network-leaf { display: grid; justify-items: center; align-content: center; gap: 8px; padding: 13px 3px 11px; text-align: center; }
.network-leaf b { font-size: 11px; }
.network-leaf code { font-size: 9px; color: #a58e97; }
.flow-arrow { display: block; width: 100%; min-width: 12px; height: 20px; color: #bc897d; }
.feature-illustration:dir(rtl) .flow-arrow { transform: scaleX(-1); }
.working-mark { flex-shrink: 0; color: #6ba8ff; }
.stage-symbol { color: #aaa0ac; }
.is-current .stage-symbol { color: #dbb59e; }
.feature-illustration:dir(rtl) .is-current .stage-symbol { transform: scaleX(-1); }
@media (max-width: 600px) {
  .network-trunk { height: 10px; }
  .network-branches { width: calc(50% - 10px); height: 10px; margin-inline-start: 10px; margin-inline-end: auto; border-inline-end: 0; border-start-end-radius: 0; }
  .network-branches::after { display: none; }
  .network-leaves { grid-template-columns: minmax(0, 1fr); gap: 6px; padding-inline-start: 12px; margin-inline-start: 10px; border-inline-start: 1px solid #806153; }
  .network-leaf { position: relative; grid-template-columns: 20px minmax(0, 1fr) auto; gap: 1px 8px; padding: 6px 8px; text-align: start; justify-items: start; }
  .network-leaf::before { content: ''; position: absolute; inset-inline-start: -13px; top: 50%; width: 12px; height: 1px; background: #806153; }
  .network-leaf .agent-mark { align-self: center; width: 18px; height: 18px; }
  .network-leaf > b { grid-column: 2; align-self: center; }
  .network-leaf code { grid-column: 3; }
}
@media (max-width: 380px) { .feature-illustration { padding: 20px 16px; } .has-app-view { padding: 12px; } }
</style>
