<script setup lang="ts">
import { ref } from 'vue'
import ConversationPanel from './features/conversation/ConversationPanel.vue'
import TutorPanel from './features/tutor/TutorPanel.vue'
import VocabularyPanel from './features/vocabulary/VocabularyPanel.vue'
import { languages, useSettingsStore } from './features/settings/store'
import { getRuntimeInfo, isDesktop } from './shared/desktop'

const settings = useSettingsStore()
const runtimeStatus = ref(isDesktop() ? '桌面运行环境' : '浏览器预览')
const checkingRuntime = ref(false)

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
    <header class="app-header">
      <a class="brand" href="#main-content" aria-label="Parley，跳到学习工作区">
        <img src="/app-icon.svg" width="40" height="40" alt="" />
        <div><strong>Parley</strong><span>让每一次对话，成为练习。</span></div>
      </a>
      <div class="header-status"><span class="status-dot" />Codex 尚未接入</div>
    </header>

    <main id="main-content">
      <section class="workspace-heading" aria-labelledby="workspace-title">
        <div>
          <p class="eyebrow">YOUR LANGUAGE SPACE</p>
          <h1 id="workspace-title">在对话中，慢慢熟练。</h1>
        </div>
        <div class="language-settings">
          <label
            >母语<select v-model="settings.nativeLanguage">
              <option v-for="language in languages" :key="language.code" :value="language.code">
                {{ language.label }}
              </option>
            </select></label
          >
          <span class="language-arrow" aria-hidden="true">→</span>
          <label
            >目标语言<select v-model="settings.targetLanguage">
              <option v-for="language in languages" :key="language.code" :value="language.code">
                {{ language.label }}
              </option>
            </select></label
          >
        </div>
      </section>

      <div class="workspace-grid">
        <ConversationPanel />
        <aside class="learning-sidebar" aria-label="学习辅助区">
          <TutorPanel />
          <VocabularyPanel />
        </aside>
      </div>
    </main>

    <footer class="app-footer">
      <div>
        <span class="tag">M0 · 基础工作区</span
        ><span>当前为界面骨架，语言选择仅在本次会话有效。</span>
      </div>
      <div class="runtime">
        <span role="status">{{ runtimeStatus }}</span>
        <button class="text-button" :disabled="checkingRuntime" @click="checkRuntime">
          {{ checkingRuntime ? '检查中…' : '检查桌面连接' }}
        </button>
      </div>
    </footer>
  </div>
</template>
