<script setup lang="ts">
defineProps<{
  name:
    | "apple-logo"
    | "linux-logo"
    | "windows-logo"
    | "android-logo"
    | "download-simple"
    | "tree-structure"
    | "terminal-window"
    | "plugs-connected"
    | "chat-circle-dots"
    | "plus"
    | "minus"
    | "arrow-right"
    | "arrow-bend-up-left";
}>();
</script>

<template>
  <span class="site-icon" :class="`icon-${name}`" aria-hidden="true" />
</template>

<style scoped>
:where(.site-icon) {
  display: inline-block;
  width: 24px;
  height: 24px;
  flex-shrink: 0;
  background: currentColor;
}
.icon-apple-logo {
  mask: url("../assets/icons/apple-logo.svg") center / contain no-repeat;
}
.icon-linux-logo {
  mask: url("../assets/icons/linux-logo.svg") center / contain no-repeat;
}
.icon-windows-logo {
  mask: url("../assets/icons/windows-logo.svg") center / contain no-repeat;
}
.icon-android-logo {
  mask: url("../assets/icons/android-logo.svg") center / contain no-repeat;
}
.icon-download-simple {
  mask: url("../assets/icons/download-simple.svg") center / contain no-repeat;
}
.icon-tree-structure {
  mask: url("../assets/icons/tree-structure.svg") center / contain no-repeat;
}
.icon-terminal-window {
  mask: url("../assets/icons/terminal-window.svg") center / contain no-repeat;
}
.icon-plugs-connected {
  mask: url("../assets/icons/plugs-connected.svg") center / contain no-repeat;
}
.icon-chat-circle-dots {
  mask: url("../assets/icons/chat-circle-dots.svg") center / contain no-repeat;
}
.icon-plus {
  mask: url("../assets/icons/plus.svg") center / contain no-repeat;
}
.icon-minus {
  mask: url("../assets/icons/minus.svg") center / contain no-repeat;
}
.icon-arrow-right {
  mask: url("../assets/icons/arrow-right.svg") center / contain no-repeat;
}
.icon-arrow-bend-up-left {
  mask: url("../assets/icons/arrow-bend-up-left.svg") center / contain no-repeat;
}
</style>
