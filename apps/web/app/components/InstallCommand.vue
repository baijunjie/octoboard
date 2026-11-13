<script setup lang="ts">
import { installCommand } from "../../seo/downloads";

const { t } = useSiteCopy();
const config = useRuntimeConfig();
const command = installCommand(config.public.siteUrl);
const commandId = `${useId()}-command`;
const announcement = ref("");
const copied = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined;
async function copy() {
  try {
    await navigator.clipboard.writeText(command);
  } catch {
    // The command stays selectable in the block, so a refused copy needs no message.
    return;
  }
  // Cleared first so that a second copy is announced again.
  announcement.value = "";
  copied.value = true;
  await nextTick();
  announcement.value = t("downloadCopied");
  clearTimeout(timer);
  timer = setTimeout(() => {
    copied.value = false;
    announcement.value = "";
  }, 2000);
}
onBeforeUnmount(() => clearTimeout(timer));
</script>

<template>
  <div class="install-command">
    <div class="install-command-scroll" role="region" :aria-label="t('downloadInstallLabel')" tabindex="0">
      <!-- The command reads left to right in every language. -->
      <code :id="commandId" dir="ltr">{{ command }}</code>
    </div>
    <button class="install-command-copy" type="button" :aria-describedby="commandId" @click="copy">
      {{ t(copied ? "downloadCopied" : "downloadCopy") }}
    </button>
    <span class="sr-only" role="status">{{ announcement }}</span>
  </div>
</template>

<style scoped>
.install-command {
  display: flex;
  align-items: stretch;
  border: 1px solid #333;
  background: #0c0c0c;
}
.install-command-scroll {
  flex: 1;
  min-width: 0;
  padding: 14px 16px;
  overflow-x: auto;
}
.install-command-scroll:focus-visible {
  outline: 2px solid var(--red);
  outline-offset: -2px;
}
.install-command code {
  display: block;
  white-space: nowrap;
  text-align: left;
  color: #f5f5f5;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 14px;
  line-height: 24px;
}
.install-command-copy {
  flex-shrink: 0;
  min-width: 44px;
  min-height: 44px;
  padding: 8px 16px;
  border: 0;
  border-inline-start: 1px solid #333;
  background: #181818;
  color: #f5f5f5;
  font-size: 14px;
  font-weight: 500;
  cursor: pointer;
  transition: color 200ms;
}
.install-command-copy:hover {
  color: var(--red);
}
</style>
