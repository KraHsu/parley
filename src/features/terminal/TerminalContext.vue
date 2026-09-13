<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useCodexStore } from '../codex/store'
import {
  autoSelectThread,
  studyContext,
  type TerminalSnapshot,
  type TerminalThread,
  type TerminalMessage,
} from './context'
const props = defineProps<{ startedAt: number }>()
const codex = useCodexStore()
const threads = ref<TerminalThread[]>([])
const messages = ref<TerminalMessage[]>([])
const threadId = ref('')
const selection = ref('')
const error = ref('')
const reading = ref(false)
const quote = ref<HTMLElement>()
const latest = computed(
  () =>
    [...messages.value].reverse().find((message) => message.role === 'assistant') ??
    messages.value.at(-1),
)
let generation = 0
let timer: ReturnType<typeof setTimeout> | undefined
let disposed = false
function schedule() {
  clearTimeout(timer)
  if (!disposed && codex.connected) timer = setTimeout(() => void refresh(), 2000)
}
async function refresh() {
  clearTimeout(timer)
  if (!codex.connected || disposed) return
  const current = ++generation
  const requested = threadId.value
  reading.value = true
  try {
    const result = await invoke<TerminalSnapshot>('codex_terminal_context', {
      threadId: requested || null,
    })
    if (disposed || current !== generation || !codex.connected) return
    threads.value = result.threads
    error.value = ''
    if (!requested) {
      const selected = autoSelectThread(result.threads, props.startedAt)
      if (selected) {
        threadId.value = selected
        return
      }
    }
    const previous = latest.value?.id
    messages.value = result.messages
    if (latest.value?.id !== previous) selection.value = ''
    codex.terminalContext = studyContext(messages.value, selection.value)
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
    messages.value = []
    codex.terminalContext = ''
    void refresh()
  },
  { flush: 'sync' },
)
watch(
  () => codex.connected,
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
function selectPassage() {
  const selected = window.getSelection()
  if (
    !selected ||
    !quote.value?.contains(selected.anchorNode) ||
    !quote.value.contains(selected.focusNode)
  )
    return
  selection.value = selected.toString().trim()
  codex.terminalContext = studyContext(messages.value, selection.value)
}
async function ask(mode: 'explain' | 'translate') {
  codex.tutorMode = mode
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
      >终端对话<select v-model="threadId" :disabled="!codex.connected">
        <option value="">
          {{ threads.length ? '选择会话，或等待新对话自动关联' : '等待 Codex 会话…' }}
        </option>
        <option v-for="thread in threads" :key="thread.id" :value="thread.id">
          {{ thread.title || 'Codex 对话' }}
        </option>
      </select></label
    >
    <p v-if="!threadId" class="settings-help">
      新对话会自动关联；恢复历史对话时，可从上方选择，无需复制。
    </p>
    <details v-if="latest">
      <summary>{{ selection ? '已选中片段 · 查看终端内容' : '查看终端最近回复' }}</summary>
      <blockquote ref="quote" @mouseup="selectPassage" @keyup="selectPassage">
        {{ latest.text }}
      </blockquote>
    </details>
    <div class="terminal-context-actions">
      <button
        class="secondary-button"
        :disabled="!codex.ready || !codex.terminalContext || codex.lanes.tutor.busy"
        @click="ask('explain')"
      >
        解释{{ selection ? '选中片段' : '当前回复' }}
      </button>
      <button
        class="secondary-button"
        :disabled="!codex.ready || !codex.terminalContext || codex.lanes.tutor.busy"
        @click="ask('translate')"
      >
        翻译
      </button>
      <span>{{ threadId ? '自动同步已保存的对话' : reading ? '正在关联…' : '等待关联' }}</span>
    </div>
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
  </section>
</template>
