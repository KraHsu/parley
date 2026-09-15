<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import MessageList from './features/chat/MessageList.vue'
import TutorPanel from './features/tutor/TutorPanel.vue'
import TerminalContext from './features/terminal/TerminalContext.vue'
import VocabularyPanel from './features/vocabulary/VocabularyPanel.vue'
import VocabularyEditor from './features/vocabulary/VocabularyEditor.vue'
import { useVocabularyStore } from './features/vocabulary/store'
import LanguagePicker from './features/settings/LanguagePicker.vue'
import ConnectionSettings from './features/backends/BackendSettings.vue'
import { useSettingsStore } from './features/settings/store'
import { useWorkspaceWindow } from './shared/use-workspace-window'
import { isDesktop } from './shared/desktop'

const props = defineProps<{
  initialCodexPath?: string | null
  terminalCwd?: string | null
  terminalStartedAt?: number
  terminalBackend?: 'codex' | 'claude-code'
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
const { codex, workspace, closeError, attemptClose } = useWorkspaceWindow(
  () => dialog.value?.close(),
  async () => {
    if (props.initialCodexPath) workspace.codexPath = props.initialCodexPath
    const needsSource = !!props.terminalCwd && props.terminalBackend !== 'claude-code'
    if (isDesktop()) {
      if (needsSource && workspace.codexPath) await codex.connect()
      const tutor = codex.lanes.tutor.backend
      if (tutor.kind === 'codex' && !(needsSource && tutor.profileId === 'codex-default'))
        await codex.connectCodexProfile(tutor.profileId)
    }
    if (!codex.isReady('tutor')) dialog.value?.showModal()
  },
)
watch(
  () => codex.sourceFocus,
  (focus) => {
    if (focus) view.value = focus.pane === 'tutor' ? 'tutor' : 'source-main'
  },
)
const assistantLabel = computed(() => codex.paneLabel('tutor'))
</script>
<template>
  <div class="tutor-app">
    <header class="tutor-app-header">
      <div>
        <strong>parley<span>.</span></strong>
        <p>
          {{ terminalBackend === 'claude-code' ? 'Claude Code' : 'Codex' }} 在终端，语言答疑在这里。
        </p>
      </div>
      <button class="secondary-button" @click="dialog?.showModal()">
        设置 · {{ assistantLabel }}
      </button>
    </header>
    <div v-if="workspace.loading || workspace.storageError" class="storage-banner" role="status">
      {{ workspace.loading ? '正在恢复学习记录…' : workspace.storageError }}
      <button v-if="workspace.storageError" class="text-button" @click="codex.retryStorage">
        重试
      </button>
    </div>
    <div v-if="codex.error && !codex.ready" class="storage-banner" role="alert">
      {{ codex.error }}
      <button class="text-button" @click="dialog?.showModal()">打开连接设置</button>
    </div>
    <div v-if="workspace.closing" class="storage-banner" role="status">正在保存并关闭…</div>
    <div v-if="codex.navigationError" class="storage-banner" role="alert">
      {{ codex.navigationError }}
      <button class="text-button" @click="codex.navigationError = ''">关闭提示</button>
    </div>
    <div v-if="closeError" class="storage-banner" role="alert">
      {{ closeError }}
      <button class="text-button" :disabled="workspace.closing" @click="attemptClose()">
        重试关闭
      </button>
      <button class="text-button" :disabled="workspace.closing" @click="attemptClose(true)">
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
      :backend="terminalBackend"
    />
    <main class="tutor-app-body">
      <section
        v-if="view === 'source-main'"
        class="content-panel source-conversation"
        aria-label="原对话"
      >
        <header class="panel-toolbar">
          <h2>原对话</h2>
          <button class="text-button" @click="showWords('words')">返回词句</button>
        </header>
        <div class="scroll-region" tabindex="0" aria-label="原对话消息">
          <MessageList
            pane="main"
            :messages="codex.lanes.main.messages"
            :busy="codex.lanes.main.busy"
          />
        </div>
      </section>
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
            :disabled="!workspace.initialized || workspace.closing"
          />
          <LanguagePicker
            v-model="settings.targetLanguage"
            label="目标语言"
            :disabled="!workspace.initialized || workspace.closing"
          />
          <p class="settings-help">
            这些设置用于语法助手。终端对话由官方 CLI
            管理；关联后，提问会自动附带最近对话和选中片段。
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
