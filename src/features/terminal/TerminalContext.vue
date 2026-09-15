<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useChatStore } from '../chat/store'
import { useBackendStore } from '../backends/store'
import { useSettingsStore } from '../settings/store'
import StudyText from '../vocabulary/StudyText.vue'
import type { VocabularySource } from '../vocabulary/types'
import {
  autoSelectThread,
  studyContext,
  type TerminalSnapshot,
  type TerminalThread,
  type TerminalMessage,
} from './context'
const props = defineProps<{ startedAt: number; backend?: 'codex' | 'claude-code' }>()
const codex = useChatStore()
const settings = useSettingsStore()
const backends = useBackendStore()
const sourceReady = computed(() => props.backend === 'claude-code' || codex.connected)
const sourceName = computed(() => (props.backend === 'claude-code' ? 'Claude Code' : 'Codex'))
const assistantName = computed(
  () =>
    backends.profiles.find((p) => p.id === codex.lanes.tutor.backend.profileId)?.config.name ??
    '所选助手服务',
)
const notice = ref('')
const threads = ref<TerminalThread[]>([])
const messages = ref<TerminalMessage[]>([])
const threadId = ref('')
const messageId = ref('')
const selection = ref('')
const frozen = ref(false)
const error = ref('')
const reading = ref(false)
const latest = computed(
  () =>
    messages.value.find((m) => m.id === messageId.value) ??
    [...messages.value].reverse().find((m) => m.role === 'assistant') ??
    messages.value.at(-1),
)
let generation = 0
let timer: ReturnType<typeof setTimeout> | undefined
let disposed = false
function schedule() {
  clearTimeout(timer)
  if (!disposed && sourceReady.value) timer = setTimeout(() => void refresh(), 2000)
}
async function refresh() {
  clearTimeout(timer)
  if (!sourceReady.value || disposed) return
  const current = ++generation,
    requested = threadId.value
  reading.value = true
  try {
    const result = await invoke<TerminalSnapshot>(
      props.backend === 'claude-code' ? 'claude_terminal_context' : 'codex_terminal_context',
      {
        threadId: requested || null,
      },
    )
    if (disposed || current !== generation || !sourceReady.value) return
    notice.value = result.notice ?? ''
    threads.value = result.threads
    error.value = ''
    if (!requested) {
      const selected = autoSelectThread(result.threads, props.startedAt)
      if (selected) {
        threadId.value = selected
        return
      }
    }
    if (!frozen.value) {
      const previous = latest.value?.id
      messages.value = result.messages
      if (latest.value?.id !== previous) selection.value = ''
      codex.terminalContext = studyContext(messages.value, selection.value)
    }
  } catch (e) {
    if (current === generation && !disposed) {
      error.value = String(e)
      codex.terminalContext = ''
    }
  } finally {
    if (current === generation) {
      reading.value = false
      schedule()
    }
  }
}
watch(
  threadId,
  () => {
    selection.value = ''
    messageId.value = ''
    frozen.value = false
    messages.value = []
    codex.terminalContext = ''
    void refresh()
  },
  { flush: 'sync' },
)
watch(messageId, () => {
  selection.value = ''
  frozen.value = false
  codex.terminalContext = studyContext(latest.value ? [latest.value] : messages.value)
})
watch(
  () => sourceReady.value,
  (connected) => {
    generation++
    clearTimeout(timer)
    if (connected) void refresh()
    else {
      reading.value = false
      codex.terminalContext = ''
    }
  },
  { immediate: true },
)
function selectedPassage(source: VocabularySource | null) {
  selection.value = source?.selectedText ?? ''
  codex.terminalContext = studyContext(messages.value, selection.value)
}
async function ask(mode: 'explain' | 'translate') {
  codex.tutorMode = mode
  codex.terminalContext = studyContext(
    latest.value ? [latest.value] : messages.value,
    selection.value,
  )
  await codex.send(
    'tutor',
    mode === 'translate'
      ? '请翻译当前终端内容；若有选中片段，优先翻译选中部分。'
      : '请解释当前终端内容中的词义和语法；若有选中片段，重点解释选中部分。',
    mode,
  )
}
onUnmounted(() => {
  disposed = true
  generation++
  clearTimeout(timer)
  codex.terminalContext = ''
})
</script>
<template>
  <section class="terminal-context" aria-label="自动同步的终端上下文">
    <label
      >终端对话<select v-model="threadId" :disabled="!sourceReady">
        <option value="">
          {{ threads.length ? '选择会话，或等待新对话自动关联' : `等待 ${sourceName} 会话…` }}
        </option>
        <option v-for="thread in threads" :key="thread.id" :value="thread.id">
          {{ thread.title || `${sourceName} 对话` }}
        </option>
      </select></label
    >
    <p v-if="!threadId" class="settings-help">
      新对话会自动关联；恢复历史对话时，从上方选择，无需复制。
    </p>
    <p v-if="notice" class="settings-help" role="status">{{ notice }}</p>
    <p class="settings-help">
      解释或翻译将发送到 {{ assistantName }}（{{ codex.tutorModel || '待选择模型' }}）。
    </p>
    <details v-if="latest">
      <summary>{{ selection ? '已选中片段 · 查看终端内容' : '查看终端最近消息' }}</summary>
      <label
        >查看消息<select v-model="messageId">
          <option value="">跟随最近回复</option>
          <option v-for="message in messages" :key="message.id" :value="message.id">
            {{ message.role === 'assistant' ? sourceName : '你' }} · {{ message.text.slice(0, 45) }}
          </option>
        </select></label
      >
      <StudyText
        :key="`${threadId}:${latest.id}`"
        :text="latest.text"
        :language="settings.targetLanguage"
        :origin="{
          sourceKind: 'terminal',
          conversationId: null,
          messageId: null,
          threadId: latest.threadId ?? threadId,
          turnId: latest.turnId ?? null,
          itemId: latest.id,
          role: latest.role,
          truncated: latest.truncated ?? false,
        }"
        @freeze="frozen = $event"
        @selection="selectedPassage"
      />
    </details>
    <div class="terminal-context-actions">
      <button
        class="secondary-button"
        :disabled="!codex.isReady('tutor') || !codex.terminalContext || codex.lanes.tutor.busy"
        @click="ask('explain')"
      >
        解释{{ selection ? '选中片段' : '当前回复' }}</button
      ><button
        class="secondary-button"
        :disabled="!codex.isReady('tutor') || !codex.terminalContext || codex.lanes.tutor.busy"
        @click="ask('translate')"
      >
        翻译</button
      ><span>{{ threadId ? '自动同步终端对话' : reading ? '正在关联…' : '等待关联' }}</span>
    </div>
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
  </section>
</template>
