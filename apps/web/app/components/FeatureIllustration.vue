<script setup lang="ts">
defineProps<{ feature: string }>();
const { t } = useSiteCopy();
const { locale } = useI18n();
const appViews = ["managedAccounts", "usageMonitor", "sessionStatus", "existingWorkflow", "reviewChanges"];
const ids = ["A", "B", "C"];
const brands = ["Claude Code", "Codex", "Grok Build"];
const agents = ["claude", "codex", "grok"];
const quota = [62, 35, 0];
const account = (id: string) => t("featureVisualAccount", { id: `\u2068${id}\u2069` });
const project = (id: string) => t("featureVisualProject", { id: `\u2068${id}\u2069` });
const number = (value: number) => new Intl.NumberFormat(locale.value).format(value);
const percent = (value: number) => new Intl.NumberFormat(locale.value, { style: "percent", maximumFractionDigits: 0 }).format(value / 100);
</script>

<template>
  <div class="feature-illustration" :class="[`illustration-${feature}`, { 'has-app-view': appViews.includes(feature) }]" aria-hidden="true">
    <FeatureAppView v-if="appViews.includes(feature)" :feature="feature" />

    <div v-else-if="feature === 'switchAccount'" class="switch-scene">
      <div class="provider-heading"><FeatureAgentMark agent="claude" /><b>Claude Code</b><span class="chip">{{ t('featureVisualContext') }}</span></div>
      <div class="switch-track">
        <div class="account-tile visual-node"><FeatureGlyph name="account" /><b>{{ account('A') }}</b><span class="small-meter"><i style="width: 56%" /></span></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="account-tile visual-node is-accent"><span class="selected-check"><FeatureGlyph name="check" :size="12" /></span><FeatureGlyph name="account" /><b>{{ account('B') }}</b><span class="small-meter"><i style="width: 76%" /></span></div>
      </div>
      <div class="retained-session"><FeatureGlyph name="chat" :size="17" /><span>{{ t('accountExampleContext') }}</span><FeatureGlyph name="context" class="context-breadcrumb" :size="17" /><FeatureGlyph name="check" :size="14" /></div>
    </div>

    <div v-else-if="feature === 'smartAccount'" class="selection-scene visual-surface">
      <div class="surface-heading"><span>{{ t('featureVisualAuto') }}</span><FeatureGlyph name="branch" :size="15" /><b>{{ t('featureVisualNext') }}</b></div>
      <div v-for="(id, index) in ids" :key="id" class="selection-row" :class="{ 'selected-row': index === 1, 'muted-row': index === 2 }">
        <span class="selection-tick"><FeatureGlyph v-if="index === 1" name="check" :size="14" /><FeatureGlyph v-else name="account" :size="16" /></span>
        <div class="selection-name"><b>{{ account(id) }}</b><span>{{ t(index === 0 ? 'featureVisualLater' : index === 1 ? 'featureVisualSooner' : 'featureVisualLimit') }}</span></div>
        <div class="selection-value"><b>{{ percent(quota[index]!) }}</b><span class="small-meter"><i :style="{ width: `${quota[index]}%` }" /></span></div>
      </div>
    </div>

    <div v-else-if="feature === 'autoSwitch'" class="fallback-scene">
      <div class="provider-heading"><FeatureGlyph name="chat" :size="18" /><b>{{ t('featureVisualTask') }}</b><code>order.ts</code><span class="chip">{{ t('featureVisualAuto') }}</span></div>
      <div class="fallback-route">
        <div class="fallback-account visual-node muted-row"><FeatureGlyph name="account" /><b>{{ account('A') }}</b><span class="fallback-state">{{ t('featureVisualLimit') }}</span></div>
        <div class="fallback-connector"><FeatureGlyph name="arrow-down" class="vertical-arrow" :size="14" /></div>
        <div class="fallback-account visual-node is-accent"><FeatureGlyph name="account" /><b>{{ account('B') }}</b><span class="fallback-state">{{ t('featureVisualContinue') }}</span><FeatureGlyph name="working" class="working-mark" :size="10" /></div>
      </div>
      <div class="continuity-thread"><span /><i /><i /><i /><span /></div>
    </div>

    <div v-else-if="feature === 'quotaRecovery'" class="recovery-scene">
      <div class="recovery-stages">
        <div class="recovery-stage"><FeatureGlyph name="pause-circle" class="stage-symbol" :size="36" /><span>{{ t('featureVisualWaiting') }}</span></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="recovery-stage"><FeatureGlyph name="check-circle" class="stage-symbol" :size="36" /><span>{{ t('featureVisualCheckQuota') }}</span></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="recovery-stage is-current"><FeatureGlyph name="play-circle" class="stage-symbol" :size="36" /><span>{{ t('featureVisualResume') }}</span></div>
      </div>
      <div class="recovery-task visual-surface"><div class="task-bar"><FeatureGlyph name="code" :size="16" /><span>{{ t('featureVisualTask') }}</span><FeatureGlyph name="working" class="working-mark" :size="10" /></div><div class="terminal-snippet"><FeatureGlyph name="prompt" :size="13" /><code>pnpm test</code><span class="terminal-cursor" /></div></div>
    </div>

    <div v-else-if="feature === 'contextWindow'" class="context-scene visual-surface">
      <div class="surface-heading"><FeatureGlyph name="chat" :size="16" /><b>{{ t('featureVisualContext') }}</b><FeatureGlyph name="more" class="window-dots" :size="17" /></div>
      <div class="context-body"><div class="context-pages"><div class="context-page back-page" /><div class="context-page front-page"><FeatureGlyph name="file" :size="17" /><span class="sheet-lines"><i /><i /><i /></span></div></div><div class="context-gauge"><strong>{{ percent(82) }}</strong></div></div>
      <div class="context-footer"><bdi>{{ number(164000) }} / {{ number(200000) }}</bdi><span>{{ t('featureVisualNext') }}<FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" /></span></div>
    </div>

    <div v-else-if="feature === 'conversationRelay'" class="relay-scene">
      <div class="relay-sessions">
        <div class="session-sheet visual-node"><div class="mini-heading"><FeatureGlyph name="chat" :size="14" /><b>{{ t('featureVisualEarlier') }}</b></div><span class="message-bubble"><i /><i /></span><span class="message-bubble reply-bubble"><i /><i /></span></div>
        <FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" />
        <div class="session-sheet visual-node is-accent"><div class="mini-heading"><FeatureGlyph name="chat" :size="14" /><b>{{ t('featureVisualNext') }}</b></div><div class="brief-note"><FeatureGlyph name="file" :size="17" /><span>{{ t('handoffExampleBrief') }}</span></div></div>
      </div>
      <div class="relay-bottom"><span class="history-branch"><FeatureGlyph name="history" :size="15" />{{ t('featureVisualHistory') }}</span><span class="relay-brief"><FeatureGlyph name="check" :size="13" />{{ t('featureVisualContinue') }}</span></div>
    </div>

    <div v-else-if="feature === 'agentHandoff'" class="handoff-scene">
      <div class="handoff-agents"><div class="agent-card visual-node"><FeatureAgentMark agent="claude" /><b>Claude Code</b><span class="agent-card-rule" /></div><div class="handoff-packet"><FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" /><span class="packet-sheet"><FeatureGlyph name="file" :size="25" /></span><FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" /></div><div class="agent-card visual-node is-accent"><FeatureAgentMark agent="codex" /><b>Codex</b><span class="agent-card-rule" /></div></div>
      <div class="brief-strip"><FeatureGlyph name="file" :size="16" /><span>{{ t('handoffExampleBrief') }}</span><FeatureGlyph name="check" :size="14" /></div>
    </div>

    <div v-else-if="feature === 'teamCoordination'" class="network-scene">
      <div class="network-head"><span class="network-direction">{{ t('featureVisualAssign') }}<FeatureGlyph name="arrow-down" class="vertical-arrow" :size="13" /></span><div class="network-root visual-node is-accent"><FeatureGlyph name="coordinator" :size="17" /><b>{{ t('demoCoordinator') }}</b></div><span class="network-direction"><FeatureGlyph name="arrow-up" class="vertical-arrow" :size="13" />{{ t('featureVisualReport') }}</span></div>
      <div class="network-trunk" /><div class="network-branches" />
      <div class="network-leaves"><div v-for="(brand, index) in brands" :key="brand" class="network-leaf visual-node"><FeatureAgentMark :agent="agents[index]!" /><b>{{ brand }}</b><span class="work-label">{{ t(['featureVisualImplementation', 'featureVisualChecks', 'featureVisualDocumentation'][index]!) }}</span><code>{{ ['order.ts', 'order.test.ts', 'README.md'][index] }}</code></div></div>
    </div>

    <div v-else-if="feature === 'projectCoordination'" class="dependencies-scene">
      <div class="dependencies-heading"><FeatureGlyph name="branch" :size="16" />{{ t('featureVisualDependencies') }}</div>
      <div class="dependency-projects"><template v-for="(id, index) in ids" :key="id"><FeatureGlyph v-if="index" name="arrow-right" class="flow-arrow" :size="20" :class="{ 'reverse-arrow': index === 2 }" /><div class="dependency-project visual-node" :class="{ 'is-accent': index === 1 }"><FeatureGlyph name="folder" :size="22" /><b>{{ project(id) }}</b><code>{{ ['web/', 'api/', 'app/'][index] }}</code><span class="project-files"><i /><i /></span></div></template></div>
      <div class="dependency-bracket" /><div class="network-trunk" /><div class="dependency-coordinator"><FeatureGlyph name="coordinator" :size="16" /><b>{{ t('demoCoordinator') }}</b></div>
    </div>

    <div v-else-if="feature === 'directParticipation'" class="participation-scene">
      <div class="participation-actor visual-node"><FeatureGlyph name="coordinator" :size="18" /><b>{{ t('demoCoordinator') }}</b></div>
      <div class="participation-route"><span>{{ t('featureVisualReport') }}</span><FeatureGlyph name="arrow-both" class="two-way-arrow" :size="22" /></div>
      <div class="participation-project visual-node is-accent"><FeatureAgentMark agent="codex" /><b>{{ project('A') }}</b><span class="executor-rule" /><span class="executor-file"><FeatureGlyph name="file" :size="13" /><code>order.ts</code></span><FeatureGlyph name="working" class="working-mark" :size="10" /></div>
      <div class="participation-actor visual-node"><FeatureGlyph name="account" :size="18" /><b>{{ t('demoYou') }}</b></div>
      <div class="participation-route direct-route"><span>{{ t('featureVisualDirect') }}</span><FeatureGlyph name="arrow-right" class="flow-arrow" :size="20" /></div>
    </div>

    <div v-else-if="feature === 'separateProjectContext'" class="isolation-scene">
      <div v-for="(id, index) in ids.slice(0, 2)" :key="id" class="project-boundary"><div class="boundary-heading"><FeatureGlyph name="folder" :size="18" /><b>{{ project(id) }}</b><FeatureGlyph name="shield" :size="14" /></div><div class="boundary-files"><span><FeatureGlyph name="file" :size="13" /><code>{{ index ? 'AGENTS.md' : 'CLAUDE.md' }}</code></span><span><FeatureGlyph name="folder" :size="13" /><code>{{ index ? '.codex/' : '.claude/' }}</code></span></div><div class="context-token"><FeatureGlyph name="chat" :size="16" /><span>{{ t('featureVisualContext') }}</span><FeatureGlyph name="context" class="context-breadcrumb" :size="17" /></div></div>
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
.muted-row { color: #9c939c; background: #1d1b1f; }
.small-meter { display: block; height: 3px; overflow: hidden; border-radius: 4px; background: #484047; }
.small-meter i { display: block; height: 100%; border-radius: inherit; background: #bd8276; }
.provider-heading, .mini-heading { display: flex; align-items: center; gap: 8px; min-width: 0; }
.provider-heading code { color: #9f909a; }
.provider-heading .chip { margin-inline-start: auto; }
.chip { padding: 3px 7px; border: 1px solid #65483f; border-radius: 5px; color: #cba094; font-size: 11px; }
.surface-heading { display: flex; align-items: center; gap: 8px; padding: 11px 14px; border-bottom: 1px solid #ffffff0b; color: #c8b7bd; font-size: 11px; }
.surface-heading > b:last-child { margin-inline-start: auto; }
.switch-scene { display: grid; gap: 15px; }
.switch-track { display: grid; grid-template-columns: minmax(0, 1fr) 28px minmax(0, 1fr); align-items: center; gap: 10px; }
.account-tile { position: relative; display: grid; justify-items: center; align-content: center; gap: 10px; padding: 14px 6px; min-height: 94px; text-align: center; }
.account-tile > svg { color: #bdabb4; }
.account-tile .small-meter { width: 55%; }
.selected-check { position: absolute; inset-inline-end: 7px; top: 7px; color: #dfb0a2; }
.retained-session { display: flex; align-items: center; gap: 8px; padding: 10px; border: 1px solid #51413e; border-radius: 7px; background: #221e21; font-size: 11px; }
.retained-session > span:nth-child(2) { flex: 1; }
.retained-session > svg:last-child { color: #caa08b; }
.selection-scene { overflow: hidden; }
.selection-row { display: flex; align-items: center; gap: 10px; padding: 11px 13px; border-inline-start: 2px solid transparent; }
.selection-row + .selection-row { border-top: 1px solid #ffffff07; }
.selected-row { background: #49322b55; border-inline-start-color: #cb9178; }
.selection-tick { display: grid; place-items: center; width: 17px; color: #a796a1; flex-shrink: 0; }
.selected-row .selection-tick { color: #e3b198; }
.selection-name { display: grid; gap: 2px; flex: 1; min-width: 0; }
.selection-name > span { font-size: 11px; color: #a69ba3; overflow-wrap: anywhere; }
.selection-value { display: grid; gap: 6px; min-width: 42px; text-align: end; }
.fallback-scene { display: grid; gap: 16px; }
.fallback-route { padding-inline-start: 16px; position: relative; }
.fallback-route::before { content: ''; position: absolute; inset-inline-start: 0; top: 25px; bottom: 25px; width: 16px; border: 1px solid #94685b; border-inline-end: 0; border-radius: 8px 0 0 8px; }
.feature-illustration:dir(rtl) .fallback-route::before { border-radius: 0 8px 8px 0; }
.fallback-account { display: flex; align-items: center; gap: 8px; padding: 13px 10px; min-height: 52px; }
.fallback-state { margin-inline-start: auto; text-align: end; font-size: 11px; }
.fallback-connector { height: 20px; display: grid; place-items: center; }
.continuity-thread { display: flex; align-items: center; gap: 5px; margin-inline-start: 17px; }
.continuity-thread span { height: 1px; background: #57423d; flex: 1; }
.continuity-thread i { width: 4px; height: 4px; border-radius: 50%; background: #a98378; }
.recovery-scene { display: grid; gap: 26px; }
.recovery-stages { display: grid; grid-template-columns: minmax(0, 1fr) 14px minmax(0, 1.2fr) 14px minmax(0, 1fr); align-items: start; gap: 5px; }
.recovery-stages > .flow-arrow { margin-top: 8px; }
.recovery-stage { display: grid; justify-items: center; gap: 10px; min-width: 0; text-align: center; font-size: 11px; overflow-wrap: anywhere; }
.recovery-task { padding: 11px 13px; }
.task-bar { display: flex; align-items: center; gap: 8px; font-size: 11px; color: #bfaeb6; }
.task-bar .working-mark { margin-inline-start: auto; }
.terminal-snippet { display: flex; align-items: center; gap: 9px; margin-top: 12px; direction: ltr; color: #cab8ad; }
.terminal-snippet > svg { color: #92828d; }
.terminal-cursor { display: block; width: 5px; height: 12px; background: #b99683; }
.context-scene { overflow: hidden; }
.context-body { display: flex; align-items: center; justify-content: space-between; padding: 19px 19px 16px; }
.context-pages { position: relative; width: 105px; height: 82px; }
.context-page { position: absolute; display: grid; align-content: center; gap: 10px; width: 79px; height: 76px; padding: 12px; border: 1px solid #68535a; border-radius: 6px; background: #30272d; }
.back-page { inset-inline-start: 0; top: 0; transform: rotate(-8deg); background: #211f26; }
.front-page { inset-inline-start: 17px; top: 5px; transform: rotate(3deg); }
.sheet-lines { display: grid; gap: 5px; }
.sheet-lines i { height: 2px; width: 90%; background: #a17a73; border-radius: 2px; }
.sheet-lines i:nth-child(2) { width: 65%; background: #79666f; }
.context-gauge { display: grid; position: relative; place-items: center; width: 75px; height: 75px; border-radius: 50%; background: conic-gradient(#c58b76 0 82%, #48383b 82% 100%); }
.context-gauge::before { content: ''; position: absolute; inset: 4px; border-radius: inherit; background: #242025; box-shadow: 0 0 0 3px #1c191e; }
.context-gauge strong { position: relative; font-size: 21px; font-weight: 400; color: #e1c3b5; }
.context-footer { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 10px 13px; border-top: 1px solid #ffffff09; font-size: 11px; }
.context-footer > span { display: flex; align-items: center; gap: 8px; }
.context-footer .flow-arrow { width: 13px; }
.relay-scene { display: grid; gap: 19px; }
.relay-sessions { display: grid; grid-template-columns: minmax(0, 1fr) 20px minmax(0, 1fr); align-items: center; gap: 8px; }
.session-sheet { display: grid; align-content: start; gap: 10px; min-height: 128px; padding: 11px 9px; }
.mini-heading { padding-bottom: 8px; border-bottom: 1px solid #ffffff0c; }
.mini-heading b { font-size: 11px; }
.message-bubble { display: grid; gap: 5px; padding: 8px; border: 1px solid #49404a; border-radius: 5px 5px 5px 0; background: #302a32; width: 82%; }
.message-bubble i { height: 2px; background: #8d7c88; border-radius: 2px; }
.message-bubble i:last-child { width: 65%; }
.reply-bubble { margin-inline-start: auto; background: #382c2c; border-color: #5b4240; border-radius: 5px 5px 0 5px; }
.reply-bubble i { background: #a58274; }
.brief-note { display: grid; justify-items: start; gap: 7px; padding: 8px; border: 1px solid #775349; border-radius: 5px; background: #392a27; color: #cfb3a4; font-size: 11px; }
.relay-bottom { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 35px; align-items: center; font-size: 11px; }
.history-branch { display: flex; align-items: center; gap: 6px; position: relative; border: 1px dashed #62505b; border-radius: 5px; padding: 8px; }
.history-branch::before { content: ''; position: absolute; width: 1px; height: 19px; top: -20px; inset-inline-start: 20px; background: #62505b; }
.relay-brief { display: flex; align-items: center; gap: 6px; color: #c3a18e; }
.handoff-scene { display: grid; gap: 24px; }
.handoff-agents { display: grid; grid-template-columns: minmax(0, 1fr) 63px minmax(0, 1fr); align-items: center; gap: 8px; }
.agent-card { display: grid; justify-items: center; gap: 12px; padding: 17px 5px 13px; text-align: center; }
.agent-card .feature-agent-mark { width: 28px; height: 28px; }
.agent-card-rule { width: 26px; height: 2px; background: #705954; border-radius: 3px; }
.handoff-packet { display: flex; align-items: center; gap: 3px; }
.handoff-packet .flow-arrow { flex: 1; min-width: 8px; }
.packet-sheet { display: grid; place-items: center; width: 31px; height: 39px; border: 1px solid #a77a62; background: #3a2d27; border-radius: 5px; color: #d6af91; transform: rotate(5deg); flex-shrink: 0; box-shadow: 3px 3px #19171c; }
.brief-strip { display: flex; justify-content: center; align-items: center; gap: 9px; padding: 11px 8px; border: 1px solid #483b3d; border-radius: 6px; background: #221e22; font-size: 11px; text-align: center; }
.brief-strip > svg { color: #b99581; }
.network-head { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.7fr) minmax(0, 1fr); align-items: center; gap: 7px; }
.network-root { display: flex; align-items: center; justify-content: center; gap: 6px; padding: 10px 5px; text-align: center; }
.network-root b { font-size: 11px; }
.network-direction { display: flex; justify-content: center; align-items: center; gap: 5px; color: #ae9ca6; font-size: 11px; text-align: center; }
.network-trunk { width: 1px; height: 17px; margin-inline: auto; background: #946d5f; }
.network-branches { height: 12px; width: 69%; margin-inline: auto; border: 1px solid #806153; border-bottom: 0; border-radius: 5px 5px 0 0; position: relative; }
.network-branches::after { content: ''; position: absolute; top: 0; left: 50%; width: 1px; height: 12px; background: #806153; }
.network-leaves { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 7px; }
.network-leaf { display: grid; justify-items: center; align-content: center; gap: 8px; padding: 13px 3px 11px; text-align: center; }
.network-leaf b { font-size: 11px; }
.network-leaf code { font-size: 9px; color: #a58e97; }
.work-label { font-size: 11px; color: #c1aaad; overflow-wrap: anywhere; }
.dependencies-scene { display: grid; justify-items: center; }
.dependencies-heading { display: flex; align-items: center; gap: 8px; margin-bottom: 17px; color: #c5b2b6; font-size: 12px; }
.dependency-projects { display: grid; grid-template-columns: minmax(0, 1fr) 16px minmax(0, 1fr) 16px minmax(0, 1fr); align-items: center; width: 100%; gap: 5px; }
.dependency-project { display: grid; justify-items: center; gap: 8px; padding: 13px 4px 11px; width: 100%; text-align: center; }
.dependency-project > svg { color: #bd9d88; }
.dependency-project code { color: #c7ada2; }
.project-files { display: grid; gap: 4px; width: 43%; margin-top: 2px; }
.project-files i { height: 2px; background: #77606a; border-radius: 1px; }
.project-files i:last-child { width: 65%; }
.reverse-arrow { transform: rotate(180deg); }
.dependency-bracket { width: 73%; height: 15px; border: 1px solid #806153; border-top: 0; border-radius: 0 0 5px 5px; }
.dependencies-scene .network-trunk { height: 10px; }
.dependency-coordinator { display: flex; align-items: center; gap: 8px; padding: 7px 12px; }
.isolation-scene { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 14px; }
.project-boundary { display: grid; gap: 14px; padding: 13px 10px; border: 1px dashed #876657; border-radius: 9px; background: #2b242455; }
.project-boundary:nth-child(2) { border-color: #656a87; background: #23263355; }
.boundary-heading { display: flex; align-items: center; gap: 5px; }
.boundary-heading > svg:last-child { margin-inline-start: auto; color: #ad9686; }
.boundary-files { display: grid; gap: 9px; padding-block: 11px; border-block: 1px solid #ffffff0a; }
.boundary-files > span { display: flex; align-items: center; gap: 6px; }
.boundary-files code { font-size: 10px; color: #c1adb6; }
.context-token { display: flex; align-items: center; gap: 7px; font-size: 11px; color: #c1afb7; }
.context-token > span { min-width: 0; overflow-wrap: anywhere; }
.context-token .context-breadcrumb { margin-inline-start: auto; }
.participation-scene { display: grid; grid-template-columns: minmax(0, 1fr) 60px minmax(0, 1fr); grid-template-rows: repeat(2, 68px); align-items: center; gap: 20px 9px; }
.participation-actor { display: flex; align-items: center; justify-content: center; gap: 7px; align-self: stretch; padding: 9px 5px; text-align: center; }
.participation-project { display: grid; justify-items: center; align-content: center; gap: 11px; grid-column: 3; grid-row: 1 / 3; align-self: stretch; padding: 12px 7px; text-align: center; }
.participation-route { display: grid; gap: 12px; font-size: 11px; text-align: center; color: #b69ea9; }
.direct-route { color: #d1a28c; }
.executor-file { display: flex; align-items: center; gap: 5px; color: #c3a9ae; }
.executor-rule { width: 72%; height: 1px; background: #5f4342; }
.flow-arrow { display: block; width: 100%; min-width: 12px; height: 20px; color: #bc897d; }
.feature-illustration:dir(rtl) .flow-arrow { transform: scaleX(-1); }
.feature-illustration:dir(rtl) .reverse-arrow { transform: rotate(180deg) scaleX(-1); }
.vertical-arrow { flex-shrink: 0; color: #b88b77; }
.working-mark { flex-shrink: 0; color: #6ba8ff; }
.context-breadcrumb { flex-shrink: 0; color: #b28277; }
.stage-symbol { color: #aaa0ac; }
.is-current .stage-symbol { color: #dbb59e; }
.feature-illustration:dir(rtl) .is-current .stage-symbol { transform: scaleX(-1); }
.window-dots { margin-inline-start: auto; color: #8d7f87; }
.two-way-arrow { width: 100%; height: 22px; color: #bb8e7a; }
.feature-illustration:dir(rtl) .two-way-arrow { transform: scaleX(-1); }
.project-boundary:nth-child(2) .context-breadcrumb { color: #8793b8; }
@media (max-width: 600px) {
  .network-trunk { height: 10px; }
  .network-branches { width: calc(50% - 10px); height: 10px; margin-inline-start: 10px; margin-inline-end: auto; border-inline-end: 0; }
  .network-branches::after { display: none; }
  .network-leaves { grid-template-columns: minmax(0, 1fr); gap: 6px; padding-inline-start: 12px; margin-inline-start: 10px; border-inline-start: 1px solid #806153; }
  .network-leaf { position: relative; grid-template-columns: 20px minmax(0, 1fr) minmax(0, 1fr); grid-template-rows: auto auto; gap: 1px 8px; padding: 6px 8px; text-align: start; justify-items: start; }
  .network-leaf::before { content: ''; position: absolute; inset-inline-start: -13px; top: 50%; width: 12px; height: 1px; background: #806153; }
  .network-leaf .feature-agent-mark { grid-row: 1 / 3; align-self: center; width: 18px; height: 18px; }
  .network-leaf > b { grid-column: 2; grid-row: 1 / 3; align-self: center; }
  .network-leaf .work-label { grid-column: 3; grid-row: 1; overflow-wrap: normal; hyphens: auto; }
  .network-leaf code { grid-column: 3; grid-row: 2; }
}
@media (max-width: 380px) { .feature-illustration { padding: 20px 16px; } .has-app-view { padding: 12px; } .provider-heading code { display: none; } .network-direction { font-size: 10px; } .isolation-scene { gap: 9px; } .project-boundary { padding: 12px 7px; } .boundary-heading { flex-wrap: wrap; }  }
</style>
