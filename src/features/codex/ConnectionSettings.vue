<script setup lang="ts">
import { computed } from 'vue'
import { useChatStore } from '../chat/store'
defineProps<{ tutorOnly?: boolean }>()
const codex = useChatStore()
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
      连接时自动读取本机 Codex 已有的登录状态。两处对话使用你的 ChatGPT 账号及其 Codex 额度。
    </p>
    <label class="settings-field codex-path-field">
      Codex 可执行文件路径
      <input
        v-model="codex.codexPath"
        type="text"
        placeholder="粘贴你安装的 Codex 的绝对路径"
        :disabled="codex.connected || codex.connecting || !codex.initialized"
        spellcheck="false"
        autocomplete="off"
        aria-describedby="codex-path-help"
      />
    </label>
    <p id="codex-path-help" class="settings-help">
      macOS / Linux：在终端运行 <code>command -v codex</code>，复制完整路径。 Windows：填写
      <code>codex.exe</code> 的完整路径。路径会自动保存；更换前请先断开连接。
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
          v-if="codex.needsLogin"
          class="primary-button"
          :disabled="codex.loggingIn"
          @click="codex.signIn"
        >
          {{ codex.loggingIn ? '等待浏览器登录…' : '登录 ChatGPT' }}
        </button>
        <button
          class="secondary-button"
          :disabled="codex.checkingAccount || codex.loadingModels"
          @click="codex.refresh"
        >
          {{
            codex.checkingAccount
              ? '读取本机账号…'
              : codex.loadingModels
                ? '读取模型…'
                : '读取本机登录 / 刷新'
          }}
        </button>
        <button class="secondary-button" @click="codex.disconnect">断开连接</button>
      </template>
    </div>
    <div v-if="codex.login" class="login-pending">
      <p>请在浏览器中完成 OpenAI 登录。若已完成，可点击“读取本机登录 / 刷新”。</p>
      <button class="secondary-button" @click="codex.openLogin">重新打开登录页</button>
      <button class="secondary-button" @click="codex.cancelLogin">取消登录</button>
    </div>
    <p v-if="codex.error" class="inline-error" role="alert">{{ codex.error }}</p>
    <p v-if="codex.notice" class="settings-help" role="status">{{ codex.notice }}</p>
    <p v-if="codex.limitsError" class="settings-help" role="status">{{ codex.limitsError }}</p>
    <template v-if="codex.account?.type === 'chatgpt' && codex.models.length">
      <label v-if="!tutorOnly" class="settings-field"
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
