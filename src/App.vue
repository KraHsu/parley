<script setup lang="ts">
import { ref, toRef, watch } from 'vue'
import ConnectionSettings from './features/backends/BackendSettings.vue'
import ConversationPanel from './features/conversation/ConversationPanel.vue'
import TutorPanel from './features/tutor/TutorPanel.vue'
import VocabularyPanel from './features/vocabulary/VocabularyPanel.vue'
import VocabularyEditor from './features/vocabulary/VocabularyEditor.vue'
import { useVocabularyStore } from './features/vocabulary/store'
import LanguagePicker from './features/settings/LanguagePicker.vue'
import AppIcon from './shared/AppIcon.vue'
import { languages, useSettingsStore } from './features/settings/store'
import { getRuntimeInfo, isDesktop } from './shared/desktop'
import { useWorkspaceWindow } from './shared/use-workspace-window'

const settings = useSettingsStore()
const vocabulary = useVocabularyStore()
watch(
  () => vocabulary.wordFocusRequest,
  () => {
    workspace.activeView = 'vocabulary'
    workspace.mobilePane = 'main'
  },
)
const preferencesDialog = ref<HTMLDialogElement>()
const { codex, workspace, closeError, attemptClose } = useWorkspaceWindow(() =>
  preferencesDialog.value?.close(),
)
const activeView = toRef(workspace, 'activeView')
const mobilePane = toRef(workspace, 'mobilePane')
const runtimeStatus = ref(isDesktop() ? '正在使用桌面客户端' : '正在使用浏览器预览')
const checkingRuntime = ref(false)
const confirmingDelete = ref('')
async function deleteHistory(id: string) {
  if (confirmingDelete.value !== id) {
    confirmingDelete.value = id
    return
  }
  await codex.deleteConversation(id)
  confirmingDelete.value = ''
}
function showView(view: 'conversation' | 'vocabulary') {
  activeView.value = view
  mobilePane.value = 'main'
}
function openSettings() {
  preferencesDialog.value?.showModal()
}
async function checkRuntime() {
  checkingRuntime.value = true
  try {
    const info = await getRuntimeInfo()
    runtimeStatus.value = `${info.appName} ${info.appVersion} · ${info.platform}/${info.arch} · Rust 连接正常`
  } catch (error) {
    runtimeStatus.value = error instanceof Error ? error.message : String(error)
  } finally {
    checkingRuntime.value = false
  }
}
</script>

<template>
  <div class="app-shell">
    <aside class="navigation" aria-label="主导航">
      <div class="brand">
        <span class="brand-mark"><AppIcon name="chat" :size="23" /></span
        ><span>parley<span class="brand-period">.</span></span>
      </div>
      <div class="navigation-content">
        <p class="section-label navigation-label">你的学习空间</p>
        <nav>
          <button
            class="nav-item"
            :class="{ active: activeView === 'conversation' }"
            :aria-current="activeView === 'conversation' ? 'page' : undefined"
            title="对话练习"
            @click="showView('conversation')"
          >
            <AppIcon name="chat" /><span>对话练习</span><span class="nav-active-dot" /></button
          ><button
            class="nav-item"
            :class="{ active: activeView === 'vocabulary' }"
            :aria-current="activeView === 'vocabulary' ? 'page' : undefined"
            title="词句收藏"
            @click="showView('vocabulary')"
          >
            <AppIcon name="book" /><span>词句收藏</span
            ><span class="nav-count">{{ vocabulary.activeCount }}</span>
          </button>
        </nav>
        <div class="history-section">
          <p class="section-label navigation-label">最近的对话</p>
          <div class="history-scroll scroll-region" aria-label="对话记录列表" tabindex="0">
            <div v-if="!workspace.history.length" class="history-empty">
              <span class="history-line" />
              <p>每一段对话，都是进步的开始。</p>
              <span>你的对话会自动保存在本机。</span>
            </div>
            <div
              v-for="entry in workspace.history"
              :key="entry.id"
              class="history-entry"
              :class="{ selected: codex.lanes[entry.pane].id === entry.id }"
            >
              <button
                class="history-open"
                :disabled="codex.lanes[entry.pane].busy || codex.navigating"
                @click="codex.selectConversation(entry.id)"
                :title="entry.title"
              >
                <strong>{{ entry.title }}</strong
                ><span
                  >{{ entry.pane === 'main' ? '对话' : '辅导' }} ·
                  {{ new Date(entry.updatedAt).toLocaleDateString() }}</span
                >
              </button>
              <button
                class="history-delete text-button"
                :disabled="codex.lanes[entry.pane].busy || codex.navigating"
                :aria-label="`删除${entry.title}`"
                @click="deleteHistory(entry.id)"
              >
                {{ confirmingDelete === entry.id ? '确认删除' : '删除' }}
              </button>
            </div>
          </div>
        </div>
      </div>
      <div class="sidebar-bottom">
        <div class="learning-note">
          <AppIcon name="leaf" :size="18" />
          <p>不必完美，<br />只要开始表达。</p>
        </div>
        <button class="workspace-profile" @click="openSettings" title="学习设置">
          <span class="profile-avatar">P</span
          ><span class="profile-copy"
            ><strong>我的工作空间</strong><span>本地 · {{ codex.paneLabel('main') }}</span></span
          ><AppIcon name="settings" :size="17" />
        </button>
      </div>
    </aside>

    <div class="workspace">
      <header class="workspace-header">
        <div class="breadcrumb">
          <span>学习空间</span><span class="breadcrumb-divider">/</span
          ><strong>{{ activeView === 'conversation' ? '对话练习' : '词句收藏' }}</strong>
        </div>
        <div class="header-controls">
          <label class="target-selector"
            ><AppIcon name="globe" :size="16" /><span class="sr-only">目标语言</span
            ><select
              v-model="settings.targetLanguage"
              :disabled="!workspace.initialized || workspace.closing"
              aria-label="目标语言"
            >
              <option
                v-if="!languages.some((l) => l.code === settings.targetLanguage)"
                :value="settings.targetLanguage"
              >
                {{ settings.targetLanguage }}
              </option>
              <option v-for="language in languages" :key="language.code" :value="language.code">
                {{ language.label }}
              </option></select
            ><AppIcon name="chevron" :size="14" /></label
          ><span class="header-separator" /><button class="connection-button" @click="openSettings">
            <span class="tiny-dot" /><span>{{ codex.paneLabel('main') }}</span></button
          ><button class="icon-button header-settings" aria-label="学习设置" @click="openSettings">
            <AppIcon name="settings" :size="18" />
          </button>
        </div>
      </header>
      <div v-if="workspace.loading || workspace.storageError" class="storage-banner" role="status">
        <span>{{ workspace.loading ? '正在恢复本地数据…' : workspace.storageError }}</span>
        <button v-if="workspace.storageError" class="text-button" @click="codex.retryStorage">
          重试保存 / 读取
        </button>
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
      <nav class="compact-tabs" aria-label="工作区视图">
        <button
          :class="{ active: mobilePane === 'main' && activeView === 'conversation' }"
          :aria-pressed="mobilePane === 'main' && activeView === 'conversation'"
          @click="showView('conversation')"
        >
          <AppIcon name="chat" :size="16" />对话</button
        ><button
          :class="{ active: mobilePane === 'tutor' }"
          :aria-pressed="mobilePane === 'tutor'"
          @click="mobilePane = 'tutor'"
        >
          <AppIcon name="sparkles" :size="16" />语言助手</button
        ><button
          :class="{ active: mobilePane === 'main' && activeView === 'vocabulary' }"
          :aria-pressed="mobilePane === 'main' && activeView === 'vocabulary'"
          @click="showView('vocabulary')"
        >
          <AppIcon name="book" :size="16" />词句
        </button>
      </nav>
      <main class="workspace-body" :data-mobile-pane="mobilePane">
        <div class="main-panel">
          <ConversationPanel
            v-show="activeView === 'conversation'"
            @connect="openSettings"
          /><VocabularyPanel v-show="activeView === 'vocabulary'" />
        </div>
        <aside class="companion-panel" aria-label="学习辅助区"><TutorPanel /></aside>
      </main>
    </div>

    <div v-if="vocabulary.notice && activeView !== 'vocabulary'" class="word-toast" role="status">
      {{ vocabulary.notice
      }}<button class="text-button" @click="vocabulary.wordFocusRequest++">查看词句</button
      ><button class="text-button" aria-label="收起收藏提示" @click="vocabulary.notice = ''">
        ×
      </button>
    </div>
    <VocabularyEditor />
    <dialog
      ref="preferencesDialog"
      class="preferences-dialog"
      aria-labelledby="preferences-title"
      @click="
        (event) => {
          if (event.target === preferencesDialog) preferencesDialog.close()
        }
      "
    >
      <div class="dialog-heading">
        <div>
          <p class="overline">MAKE IT YOURS</p>
          <h2 id="preferences-title">学习设置</h2>
        </div>
        <button class="icon-button" aria-label="关闭设置" @click="preferencesDialog?.close()">
          <AppIcon name="close" />
        </button>
      </div>
      <div class="dialog-scroll scroll-region">
        <section class="settings-section">
          <h3>我的语言</h3>
          <LanguagePicker
            v-model="settings.nativeLanguage"
            label="母语"
            :disabled="!workspace.initialized || workspace.closing"
          /><LanguagePicker
            v-model="settings.targetLanguage"
            label="目标语言"
            :disabled="!workspace.initialized || workspace.closing"
          />
          <p class="settings-help">语言设置、模型选择和草稿会自动保存在本机。</p>
        </section>
        <ConnectionSettings />
        <section class="settings-section">
          <h3>本地数据</h3>
          <p class="settings-help" role="status">
            {{
              !isDesktop()
                ? '浏览器预览不保存数据。'
                : workspace.storageError
                  ? workspace.storageError
                  : workspace.saving
                    ? '正在保存…'
                    : '已保存到本机'
            }}
          </p>
          <p class="settings-help data-path">{{ workspace.dbPath }}</p>
          <p class="settings-help">
            可离线查看聊天记录。删除历史仅删除 Parley 本地记录；Codex 自身的会话日志不受影响。
          </p>
        </section>
        <section class="settings-section runtime-section">
          <h3>运行环境</h3>
          <p role="status">{{ runtimeStatus }}</p>
          <button class="secondary-button" :disabled="checkingRuntime" @click="checkRuntime">
            {{ checkingRuntime ? '检查中…' : '检查桌面连接' }}
          </button>
        </section>
      </div>
      <div class="dialog-footer">
        <span>Parley · 你的语言练习空间</span
        ><button class="primary-button" @click="preferencesDialog?.close()">完成</button>
      </div>
    </dialog>
  </div>
</template>
