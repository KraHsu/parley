<script setup lang="ts">
import { ref } from 'vue'
import TutorPanel from './features/tutor/TutorPanel.vue'
import TerminalContext from './features/terminal/TerminalContext.vue'
import ConnectionSettings from './features/codex/ConnectionSettings.vue'
import { languages, useSettingsStore } from './features/settings/store'
import { useWorkspaceWindow } from './shared/use-workspace-window'
import { isDesktop } from './shared/desktop'

const props = defineProps<{
  initialCodexPath?: string | null
  terminalCwd?: string | null
  terminalStartedAt?: number
}>()
const settings = useSettingsStore()
const dialog = ref<HTMLDialogElement>()
const { codex, closeError, attemptClose } = useWorkspaceWindow(
  () => dialog.value?.close(),
  async () => {
    if (props.initialCodexPath) codex.codexPath = props.initialCodexPath
    if (isDesktop() && codex.codexPath) await codex.connect()
    else dialog.value?.showModal()
  },
)
</script>
<template>
  <div class="tutor-app">
    <header class="tutor-app-header">
      <div>
        <strong>parley<span>.</span></strong>
        <p>Codex 在终端，语言答疑在这里。</p>
      </div>
      <button class="secondary-button" @click="dialog?.showModal()">
        设置 · {{ codex.label }}
      </button>
    </header>
    <div v-if="codex.loading || codex.storageError" class="storage-banner" role="status">
      {{ codex.loading ? '正在恢复学习记录…' : codex.storageError }}
      <button v-if="codex.storageError" class="text-button" @click="codex.retryStorage">
        重试
      </button>
    </div>
    <div v-if="codex.error && !codex.ready" class="storage-banner" role="alert">
      {{ codex.error }}
      <button class="text-button" @click="dialog?.showModal()">打开连接设置</button>
    </div>
    <div v-if="codex.closing" class="storage-banner" role="status">正在保存并关闭…</div>
    <div v-if="closeError" class="storage-banner" role="alert">
      {{ closeError }}
      <button class="text-button" :disabled="codex.closing" @click="attemptClose()">
        重试关闭
      </button>
      <button class="text-button" :disabled="codex.closing" @click="attemptClose(true)">
        强制关闭并放弃未保存修改
      </button>
    </div>
    <TerminalContext v-if="terminalCwd" :started-at="terminalStartedAt ?? 0" />
    <main class="tutor-app-body"><TutorPanel /></main>
    <dialog ref="dialog" class="preferences-dialog" aria-labelledby="tutor-settings-title">
      <div class="dialog-heading">
        <h2 id="tutor-settings-title">语法助手设置</h2>
        <button class="icon-button" aria-label="关闭设置" @click="dialog?.close()">×</button>
      </div>
      <div class="dialog-scroll scroll-region">
        <section class="settings-section">
          <h3>语言</h3>
          <label class="settings-field"
            >母语<select
              v-model="settings.nativeLanguage"
              :disabled="!codex.initialized || codex.closing"
            >
              <option v-for="language in languages" :key="language.code" :value="language.code">
                {{ language.label }}
              </option>
            </select></label
          >
          <label class="settings-field"
            >目标语言<select
              v-model="settings.targetLanguage"
              :disabled="!codex.initialized || codex.closing"
            >
              <option v-for="language in languages" :key="language.code" :value="language.code">
                {{ language.label }}
              </option>
            </select></label
          >
          <p class="settings-help">
            这些设置用于语法助手。终端对话由 Codex 管理；关联后，提问会自动附带最近对话和选中片段。
          </p>
        </section>
        <ConnectionSettings tutor-only />
      </div>
      <div class="dialog-footer">
        <span>辅导记录自动保存在本机</span
        ><button class="primary-button" @click="dialog?.close()">完成</button>
      </div>
    </dialog>
  </div>
</template>
