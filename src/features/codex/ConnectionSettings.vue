<script setup lang="ts">
import { computed } from 'vue'
import { useCodexStore } from './store'
const codex = useCodexStore()
const windows = computed(() => {
  const limit = codex.limits?.rateLimits
  return [limit?.primary, limit?.secondary].filter((item) => item != null)
})
function duration(minutes: number | null) {
  return minutes
    ? minutes >= 1440
      ? `${Math.round(minutes / 1440)} 天`
      : `${Math.round(minutes / 60)} 小时`
    : '额度窗口'
}
</script>
<template>
  <section class="settings-section codex-settings">
    <h3>
      Codex 连接 <span class="subtle-label">{{ codex.label }}</span>
    </h3>
    <p class="settings-help">
      使用本机 Codex CLI 和你自己的 ChatGPT 账号。两处对话消耗同一账号的 Codex 额度。
    </p>
    <p v-if="codex.account?.type === 'chatgpt'" class="account-detail">
      {{ codex.account.email ?? 'ChatGPT 账号' }} · {{ codex.account.planType }}
    </p>
    <p v-else-if="codex.account" class="inline-error">
      当前 Codex 使用 {{ codex.account.type }} 登录。请通过下方入口登录 ChatGPT 账号。
    </p>
    <div class="connection-actions">
      <button
        v-if="!codex.connected"
        class="primary-button"
        :disabled="codex.connecting"
        @click="codex.connect"
      >
        {{ codex.connecting ? '连接中…' : '连接本机 Codex' }}
      </button>
      <template v-else>
        <button
          v-if="!codex.ready"
          class="primary-button"
          :disabled="codex.loggingIn"
          @click="codex.signIn"
        >
          {{ codex.loggingIn ? '等待浏览器登录…' : '登录 ChatGPT' }}
        </button>
        <button class="secondary-button" @click="codex.refresh">刷新状态</button>
        <button class="secondary-button" @click="codex.disconnect">断开连接</button>
      </template>
    </div>
    <div v-if="codex.login" class="login-pending">
      <p>请在浏览器中完成 OpenAI 登录。登录状态由本机 Codex 管理。</p>
      <button class="secondary-button" @click="codex.openLogin">重新打开登录页</button>
      <button class="secondary-button" @click="codex.cancelLogin">取消登录</button>
    </div>
    <p v-if="codex.error" class="inline-error" role="alert">{{ codex.error }}</p>
    <p v-if="codex.notice" class="settings-help" role="status">{{ codex.notice }}</p>
    <template v-if="codex.ready">
      <label class="settings-field"
        >对话模型<select v-model="codex.mainModel" :disabled="codex.lanes.main.busy">
          <option
            v-if="codex.mainModel && !codex.models.some((m) => m.model === codex.mainModel)"
            :value="codex.mainModel"
            disabled
          >
            {{ codex.mainModel }}（当前不可用）
          </option>
          <option v-for="model in codex.models" :key="model.id" :value="model.model">
            {{ model.displayName }}
          </option>
        </select></label
      >
      <label class="settings-field"
        >辅导模型<select v-model="codex.tutorModel" :disabled="codex.lanes.tutor.busy">
          <option
            v-if="codex.tutorModel && !codex.models.some((m) => m.model === codex.tutorModel)"
            :value="codex.tutorModel"
            disabled
          >
            {{ codex.tutorModel }}（当前不可用）
          </option>
          <option v-for="model in codex.models" :key="model.id" :value="model.model">
            {{ model.displayName }}
          </option>
        </select></label
      >
      <p class="settings-help">
        模型列表来自你的账号。修改语言、模型或辅导模式后，下一条消息会开启新会话，旧记录保留在历史中。
      </p>
      <div v-for="(window, index) in windows" :key="index" class="usage-window">
        <span
          >{{ duration(window.windowDurationMins) }}额度 · 已使用 {{ window.usedPercent }}%</span
        >
        <progress
          :value="window.usedPercent"
          max="100"
          :aria-label="`${duration(window.windowDurationMins)}额度已使用`"
        />
        <small v-if="window.resetsAt"
          >重置时间：{{ new Date(window.resetsAt * 1000).toLocaleString() }}</small
        >
      </div>
    </template>
    <p class="settings-help">断开不会退出本机账号。会话和设置保存在本机，重新连接后可继续。</p>
  </section>
</template>
