<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from 'vue'
import ChatPane from './ChatPane.vue'
import SettingsDialog from './SettingsDialog.vue'
import WordDialog from './WordDialog.vue'
import WordsPanel from './WordsPanel.vue'
import { createWebWorkspace } from './store'
import type { WebState, Word } from './types'
import './web.css'
const w = createWebWorkspace()
const view = ref('chat'),
  mobilePane = ref('main')
const settings = ref<InstanceType<typeof SettingsDialog>>(),
  editor = ref<InstanceType<typeof WordDialog>>()
const file = ref<HTMLInputElement>(),
  restoreDialog = ref<HTMLDialogElement>(),
  backup = ref<WebState>()
const beforeUnload = (event: BeforeUnloadEvent) => {
  const unsaved =
    w.busy.main || w.busy.tutor || w.saving || w.dirty || (w.initialized && w.storageError)
  void w.persist()
  if (unsaved) {
    event.preventDefault()
    event.returnValue = ''
  }
}
const visibility = () => {
  if (document.visibilityState === 'hidden') void w.persist()
}
onMounted(async () => {
  await w.initialize()
  window.addEventListener('beforeunload', beforeUnload)
  document.addEventListener('visibilitychange', visibility)
})
onUnmounted(() => {
  window.removeEventListener('beforeunload', beforeUnload)
  document.removeEventListener('visibilitychange', visibility)
  w.dispose()
})
async function readBackup(event: Event) {
  const input = event.target as HTMLInputElement,
    selected = input.files?.[0]
  if (!selected) return
  try {
    if (selected.size > 32 * 1024 * 1024) throw new Error('备份不能超过 32 MiB。')
    backup.value = w.parseBackup(await selected.text())
    restoreDialog.value?.showModal()
  } catch (e) {
    w.error = e instanceof Error ? e.message : String(e)
  }
  input.value = ''
}
async function restore() {
  if (backup.value) await w.restore(backup.value)
  backup.value = undefined
  restoreDialog.value?.close()
}
async function source(word: Word) {
  const c = w.state.conversations.find((c) => c.id === word.source?.conversationId)
  if (!c) {
    w.notice = '原对话已不存在，词句中仍保留原句快照。'
    return
  }
  if (w.busy[c.pane]) {
    w.notice = '请等待该面板生成结束，或先停止生成，再查看来源。'
    return
  }
  w.selectConversation(c.pane, c.id)
  view.value = 'chat'
  mobilePane.value = c.pane
  await nextTick()
  document
    .getElementById(`web-message-${word.source!.messageId}`)
    ?.scrollIntoView({ block: 'center', behavior: 'smooth' })
}
function explain(word: Word) {
  view.value = 'chat'
  mobilePane.value = 'tutor'
  void w.send(
    'tutor',
    `请解释 ${word.text} 的词义和用法。`,
    'explain',
    word.source?.text ?? word.text,
  )
}
</script>
<template>
  <div class="web-app">
    <header class="web-header">
      <a class="brand" href="./" aria-label="Parley 首页">parley<span>.</span></a
      ><span class="web-badge">WEB · API</span>
      <nav aria-label="主导航">
        <button :class="{ active: view === 'chat' }" @click="view = 'chat'">对话</button
        ><button :class="{ active: view === 'words' }" @click="view = 'words'">
          词句 <small>{{ w.state.words.length }}</small>
        </button>
      </nav>
      <div class="header-actions">
        <span class="save-status">{{
          w.readonly ? '只读' : w.saving ? '保存中…' : '学习数据保存在此浏览器'
        }}</span
        ><button @click="settings?.show()">设置</button>
        <details class="backup-menu">
          <summary>备份</summary>
          <div>
            <button :disabled="!w.initialized" @click="w.download">导出 Web 备份</button
            ><button :disabled="!w.canEdit || w.busy.main || w.busy.tutor" @click="file?.click()">
              导入 Web 备份
            </button>
          </div>
        </details>
      </div>
    </header>
    <div v-if="w.storageError" class="web-banner danger" role="alert">
      {{ w.storageError }}<button @click="w.persist">重试保存</button
      ><button v-if="w.initialized" @click="w.download">导出当前数据</button>
    </div>
    <div v-if="w.error" class="web-banner danger" role="alert">
      {{ w.error }}<button aria-label="关闭错误提示" @click="w.error = ''">×</button>
    </div>
    <div v-if="w.notice" class="web-banner" role="status">
      {{ w.notice }}<button aria-label="关闭提示" @click="w.notice = ''">×</button>
    </div>
    <main v-if="!w.initialized" class="web-loading">
      {{
        w.storageError
          ? '无法打开本地学习数据。请检查浏览器存储权限后刷新。'
          : '正在打开你的学习空间…'
      }}
    </main>
    <template v-else
      ><section v-if="!w.state.profiles.length && view === 'chat'" class="web-welcome">
        <div>
          <h1>把对话变成语言练习。</h1>
          <p>用目标语言交流，随时理解词句。添加自己的 API，开始第一轮对话。</p>
        </div>
        <button class="primary" :disabled="!w.canEdit" @click="settings?.show()">
          添加 API 服务 →
        </button>
      </section>
      <div v-if="view === 'chat'" class="mobile-tabs segmented">
        <button :class="{ active: mobilePane === 'main' }" @click="mobilePane = 'main'">
          目标语言对话</button
        ><button :class="{ active: mobilePane === 'tutor' }" @click="mobilePane = 'tutor'">
          语言助手
        </button>
      </div>
      <div v-show="view === 'chat'" class="web-chat-grid" :data-mobile-pane="mobilePane">
        <ChatPane
          :w="w"
          pane="main"
          @tutor="mobilePane = 'tutor'"
          @collect="editor?.show($event)"
        /><ChatPane :w="w" pane="tutor" @collect="editor?.show($event)" />
      </div>
      <WordsPanel
        v-if="view === 'words'"
        :w="w"
        @edit="editor?.show($event)"
        @source="source"
        @explain="explain"
    /></template>
    <SettingsDialog ref="settings" :w="w" /><WordDialog ref="editor" :w="w" /><input
      ref="file"
      class="visually-hidden"
      type="file"
      accept="application/json,.json"
      aria-label="导入 Web 备份文件"
      @change="readBackup"
    />
    <dialog ref="restoreDialog" class="web-dialog">
      <header>
        <h2>恢复 Web 备份</h2>
        <button aria-label="取消恢复" @click="restoreDialog?.close()">×</button>
      </header>
      <div v-if="backup" class="dialog-body">
        <p>
          备份包含 {{ backup.conversations.length }} 个对话、{{ backup.words.length }} 个词句及
          {{ backup.profiles.length }} 个 API 配置。
        </p>
        <p>确认后将替换当前浏览器中的学习数据。可以先导出当前数据；密钥不会从备份恢复。</p>
        <button @click="w.download">先导出当前数据</button>
      </div>
      <footer>
        <button @click="restoreDialog?.close()">取消</button
        ><button
          class="primary"
          :disabled="!w.canEdit || w.busy.main || w.busy.tutor"
          @click="restore"
        >
          确认恢复
        </button>
      </footer>
    </dialog>
  </div>
</template>
