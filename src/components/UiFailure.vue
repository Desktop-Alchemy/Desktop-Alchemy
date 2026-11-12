<script setup lang="ts">
import { t, messageText } from "../i18n";
import type { UiMessage } from "../bindings";
defineProps<{ detail: string; message?: UiMessage | null; summary?: string }>();
</script>
<template>
  <div class="error-text" role="alert">
    <p>
      {{
        summary ||
        (message ? messageText(message) : t("errors.operationFailed"))
      }}
    </p>
    <details>
      <summary>{{ t("errors.details") }}</summary>
      <pre class="diagnostic-detail">{{ detail }}</pre>
    </details>
  </div>
</template>
<style scoped>
p {
  margin: 0;
}
pre {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  max-height: 180px;
  overflow: auto;
}
</style>
