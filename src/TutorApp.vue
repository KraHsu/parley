<script setup lang="ts">
import { ref, watch } from 'vue'
import TutorPanel from './features/tutor/TutorPanel.vue'
import TerminalContext from './features/terminal/TerminalContext.vue'
import VocabularyPanel from './features/vocabulary/VocabularyPanel.vue'
import VocabularyEditor from './features/vocabulary/VocabularyEditor.vue'
import { useVocabularyStore } from './features/vocabulary/store'
import LanguagePicker from './features/settings/LanguagePicker.vue'
import ConnectionSettings from './features/codex/ConnectionSettings.vue'
import { useSettingsStore } from './features/settings/store'
import { useWorkspaceWindow } from './shared/use-workspace-window'
import { isDesktop } from './shared/desktop'

const props = defineProps<{
  initialCodexPath?: string | null
  terminalCwd?: string | null
  terminalStartedAt?: number
}>()
const settings = useSettingsStore()
const view = ref('tutor')
const vocabulary = useVocabularyStore()
watch(
  () => vocabulary.tutorFocusRequest,
  () => (view.value = 'tutor'),
)
watch(
  () => vocabulary.wordFocusRequest,
  () => showWords('words'),
)
function showWords(tab: string) {
  vocabulary.activeTab = tab
  view.value = 'vocabulary'
}
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
    <nav class="word-companion-tabs" aria-label="语法窗口视图">
      <button class="secondary-button" :aria-pressed="view === 'tutor'" @click="view = 'tutor'">
        答疑</button
      ><button
        class="secondary-button"
        :aria-pressed="view === 'vocabulary' && vocabulary.activeTab === 'words'"
        @click="showWords('words')"
      >
        词句
      </button>
      <button
        class="secondary-button"
        :aria-pressed="view === 'vocabulary' && vocabulary.activeTab === 'review'"
        @click="showWords('review')"
      >
        复习
      </button>
    </nav>
    <TerminalContext
      v-if="terminalCwd"
      v-show="view === 'tutor'"
      :started-at="terminalStartedAt ?? 0"
    />
    <main class="tutor-app-body">
      <TutorPanel v-show="view === 'tutor'" /><VocabularyPanel
        companion
        v-show="view === 'vocabulary'"
      />
    </main>
    <div v-if="vocabulary.notice && view === 'tutor'" class="word-toast" role="status">
      {{ vocabulary.notice
      }}<button class="text-button" @click="showWords('words')">查看词句</button
      ><button class="text-button" aria-label="收起收藏提示" @click="vocabulary.notice = ''">
        ×
      </button>
    </div>
    <VocabularyEditor />
    <dialog ref="dialog" class="preferences-dialog" aria-labelledby="tutor-settings-title">
      <div class="dialog-heading">
        <h2 id="tutor-settings-title">语法助手设置</h2>
        <button class="icon-button" aria-label="关闭设置" @click="dialog?.close()">×</button>
      </div>
      <div class="dialog-scroll scroll-region">
        <section class="settings-section">
          <h3>语言</h3>
          <LanguagePicker
            v-model="settings.nativeLanguage"
            label="母语"
            :disabled="!codex.initialized || codex.closing"
          />
          <LanguagePicker
            v-model="settings.targetLanguage"
            label="目标语言"
            :disabled="!codex.initialized || codex.closing"
          />
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
