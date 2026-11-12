<script setup lang="ts">
import { t } from "../i18n";

import { onMounted, onUnmounted, ref } from "vue";
import { useDesktopAlchemy } from "../store";

interface McpStatus {
  enabled: boolean;
  port: number;
  endpoint: string;
  status: "stopped" | "running" | "error";
  error: string | null;
}
const w = useDesktopAlchemy();
const state = ref<McpStatus>({
  enabled: false,
  port: 18718,
  endpoint: "http://127.0.0.1:18718/mcp",
  status: "stopped",
  error: null,
});
const port = ref(18718);
const busy = ref(false);
const message = ref("");
let poll: ReturnType<typeof setInterval> | undefined;
async function refresh() {
  if (!w.desktop || busy.value) return;
  const result = await w.call<McpStatus>("mcp_status", {}, undefined, true);
  if (result) state.value = result;
}
async function configure(enabled: boolean) {
  if (
    enabled &&
    (!Number.isInteger(port.value) || port.value < 1 || port.value > 65535)
  ) {
    message.value = t("ui.portMustBeAnIntegerFrom");
    return;
  }
  busy.value = true;
  message.value = "";
  try {
    const result = await w.call<McpStatus>("configure_mcp", {
      enabled,
      port: enabled ? port.value : state.value.port,
    });
    if (result) {
      state.value = result;
      port.value = result.port;
    }
  } finally {
    busy.value = false;
  }
}
async function copy(value: string) {
  try {
    await navigator.clipboard.writeText(value);
    message.value = t("ui.copied");
  } catch {
    message.value = t("ui.couldNotCopyCheckClipboardPermissions");
  }
}
async function copyToken() {
  const token = await w.call<string>("mcp_token");
  if (token) await copy(token);
}
async function resetToken() {
  busy.value = true;
  try {
    const result = await w.call<McpStatus>("reset_mcp_token");
    if (result) {
      state.value = result;
      message.value = t("ui.tokenResetUpdateYourAgentConnection");
    }
  } finally {
    busy.value = false;
  }
}
onMounted(async () => {
  await refresh();
  port.value = state.value.port;
  poll = setInterval(() => void refresh(), 3000);
});
onUnmounted(() => clearInterval(poll));
</script>

<template>
  <section class="panel mcp-panel" aria-labelledby="mcp-title">
    <div class="panel-heading">
      <div>
        <h2 id="mcp-title">{{ t("ui.mcpAiDeviceControl") }}</h2>
        <p class="helper">
          {{ t("ui.letALocalAgentSetImages") }}
        </p>
      </div>
      <span class="badge" :class="state.status">{{
        {
          stopped: t("ui.off"),
          running: t("ui.running"),
          error: t("ui.connectionError"),
        }[state.status]
      }}</span>
    </div>
    <div class="mcp-connection">
      <label
        >{{ t("ui.port")
        }}<input
          v-model.number="port"
          type="number"
          min="1"
          max="65535"
          step="1"
          :disabled="!w.desktop || busy"
      /></label>
      <div class="mcp-endpoint">
        <span class="helper">{{ t("ui.localAddress") }}</span
        ><code>{{ state.endpoint }}</code>
      </div>
    </div>
    <p v-if="state.error" class="error-text" role="alert">{{ state.error }}</p>
    <div class="button-row wrap">
      <button
        class="primary"
        :disabled="!w.desktop || busy"
        @click="configure(true)"
      >
        {{
          state.status === "running"
            ? t("ui.applyPortAndRestart")
            : t("ui.enableMcp")
        }}
      </button>
      <button
        :disabled="!w.desktop || busy || !state.enabled"
        @click="configure(false)"
      >
        {{ t("ui.disable") }}
      </button>
      <button
        class="secondary"
        :disabled="!w.desktop || busy"
        @click="copy(state.endpoint)"
      >
        {{ t("ui.copyAddress") }}
      </button>
      <button
        class="secondary"
        :disabled="!w.desktop || busy"
        @click="copyToken"
      >
        {{ t("ui.copyToken") }}
      </button>
      <button class="quiet" :disabled="!w.desktop || busy" @click="resetToken">
        {{ t("ui.resetToken") }}
      </button>
    </div>
    <p class="helper">
      {{ t("ui.agentsUseStreamableHttpWithAuthorization") }}
    </p>
    <p v-if="message" class="helper" role="status">{{ message }}</p>
  </section>
</template>

<style scoped>
.mcp-panel {
  margin-bottom: 20px;
}
.mcp-connection {
  display: flex;
  flex-wrap: wrap;
  gap: 24px;
  align-items: end;
  margin: 20px 0;
}
.mcp-connection label {
  display: grid;
  gap: 8px;
  max-width: 140px;
}
.mcp-endpoint {
  display: grid;
  gap: 8px;
  padding-bottom: 8px;
  overflow-wrap: anywhere;
}
.mcp-panel > .helper {
  margin-top: 14px;
}
</style>
