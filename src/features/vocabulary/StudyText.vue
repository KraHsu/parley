<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { captureSelection, captureText, type SourceOrigin } from '../../shared/selection'
import { useVocabularyStore } from './store'
import { useCodexStore } from '../codex/store'
import type { VocabularySource } from './types'
const props = defineProps<{
  text: string
  origin: SourceOrigin
  language: string
  allowAnswer?: boolean
  answerTarget?: { id: string; requestId: string }
}>()
const emit = defineEmits<{
  freeze: [value: boolean]
  selection: [source: VocabularySource | null]
}>()
const vocabulary = useVocabularyStore()
const codex = useCodexStore()
const body = ref<HTMLElement>()
const selected = ref<VocabularySource | null>(null)
const error = ref('')
const frozen = ref(false)
const displayed = ref(props.text)
const displayedLanguage = ref(props.language)
const origin = ref({ ...props.origin })
const answerTarget = ref<{ id: string; requestId: string } | null>(null)
watch(
  () => [props.text, props.origin],
  () => {
    if (frozen.value) return
    displayed.value = props.text
    displayedLanguage.value = props.language
    origin.value = { ...props.origin }
    selected.value = null
  },
  { deep: true },
)
function begin() {
  frozen.value = true
  emit('freeze', true)
}
function select() {
  try {
    selected.value = body.value ? captureSelection(body.value, origin.value) : null
    error.value = ''
    answerTarget.value =
      props.answerTarget ??
      (vocabulary.editor
        ? { id: vocabulary.editor.id, requestId: vocabulary.editor.requestId }
        : null)
  } catch (e) {
    selected.value = null
    error.value = String(e)
  }
  emit('selection', selected.value)
  if (!selected.value) unfreeze()
}
function unfreeze() {
  frozen.value = false
  emit('freeze', false)
  emit('selection', null)
  selected.value = null
  displayed.value = props.text
  displayedLanguage.value = props.language
  origin.value = { ...props.origin }
}
function pointerReleased(event: PointerEvent) {
  if (
    frozen.value &&
    event.target instanceof Node &&
    !body.value?.contains(event.target) &&
    !selected.value
  )
    select()
}
function keyboardStarted(event: KeyboardEvent) {
  if (
    event.shiftKey &&
    ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)
  )
    begin()
}
onMounted(() => window.addEventListener('pointerup', pointerReleased))
onUnmounted(() => window.removeEventListener('pointerup', pointerReleased))
function whole() {
  try {
    selected.value = captureText(displayed.value, 0, displayed.value.length, origin.value)
    frozen.value = true
    emit('freeze', true)
    emit('selection', selected.value)
    error.value = ''
  } catch (e) {
    error.value = String(e)
  }
}
async function save() {
  if (!selected.value) whole()
  if (selected.value) await vocabulary.start(selected.value, displayedLanguage.value)
}
async function ask(mode: 'explain' | 'translate') {
  if (!selected.value) whole()
  if (!selected.value) return
  const source = JSON.parse(JSON.stringify(selected.value))
  codex.tutorMode = mode
  vocabulary.tutorFocusRequest++
  codex.mobilePane = 'tutor'
  await codex.send(
    'tutor',
    `${mode === 'translate' ? '请翻译' : '请解释词义和语法'}下面的学习材料，优先关注选中词句：\n${JSON.stringify({ language: displayedLanguage.value, selected: source.selectedText, sentence: source.snapshot })}`,
    mode,
  )
}
const answerIsStale = computed(() =>
  Boolean(
    answerTarget.value &&
    vocabulary.editor &&
    (answerTarget.value.id !== vocabulary.editor.id ||
      answerTarget.value.requestId !== vocabulary.editor.requestId),
  ),
)
function retargetAnswer() {
  if (vocabulary.editor)
    answerTarget.value = { id: vocabulary.editor.id, requestId: vocabulary.editor.requestId }
}
function useAnswer(field: 'meaning' | 'note') {
  if (selected.value && answerTarget.value)
    vocabulary.useAnswer(selected.value.selectedText, field, answerTarget.value)
}
</script>
<template>
  <div class="study-text">
    <p
      ref="body"
      dir="auto"
      class="study-body"
      tabindex="0"
      @pointerdown="begin"
      @pointerup="select"
      @keydown="keyboardStarted"
      @keyup="select"
    >
      {{ displayed }}
    </p>
    <div class="study-actions" @pointerdown.prevent>
      <button
        class="text-button"
        :disabled="!vocabulary.initialized || vocabulary.busy"
        @click="save"
      >
        {{ selected ? '收藏选中词句' : '收藏整句' }}
      </button>
      <template v-if="selected">
        <button
          class="text-button"
          :disabled="!codex.ready || codex.lanes.tutor.busy"
          @click="ask('explain')"
        >
          解释
        </button>
        <button
          class="text-button"
          :disabled="!codex.ready || codex.lanes.tutor.busy"
          @click="ask('translate')"
        >
          翻译
        </button>
        <button
          v-if="allowAnswer && answerTarget"
          class="text-button"
          @click="useAnswer('meaning')"
        >
          用作释义
        </button>
        <button v-if="allowAnswer && answerTarget" class="text-button" @click="useAnswer('note')">
          添加到注释
        </button>
        <button v-if="allowAnswer && answerIsStale" class="text-button" @click="retargetAnswer">
          确认改用于当前草稿
        </button>
        <button class="text-button" @click="unfreeze">结束选择</button>
      </template>
    </div>
    <small v-if="allowAnswer && answerIsStale" class="settings-help"
      >这份答案对应较早的词句草稿。请先核对当前词句，再确认改用。</small
    >
    <small v-if="frozen && displayed !== text" class="settings-help"
      >选区已固定，结束选择后显示最新内容。</small
    >
    <small v-if="origin.truncated" class="settings-help">当前来源为截取片段。</small>
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
  </div>
</template>
