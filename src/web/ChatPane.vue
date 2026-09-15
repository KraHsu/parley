<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import type { WebWorkspace } from './store'
import type { Message, Pane, Word } from './types'
import { tutorPlaceholders } from '../shared/tutor-placeholders'
const props = defineProps<{ w: WebWorkspace; pane: Pane }>()
const emit = defineEmits<{ collect: [word: Word]; tutor: [] }>()
const c = computed(() => props.w.lane(props.pane))
const mode = ref<keyof typeof tutorPlaceholders>('explain'),
  composing = ref(false),
  scroller = ref<HTMLElement>()
const choices = computed(() => props.w.models.get(c.value.profileId) ?? [])
const profile = computed(() => props.w.state.profiles.find((p) => p.id === c.value.profileId))
function usageText(message: Message) {
  const u = message.usage ?? {}
  return (
    [
      ['输入', u.input_tokens ?? u.prompt_tokens ?? u.total_input_tokens],
      ['输出', u.output_tokens ?? u.completion_tokens ?? u.total_output_tokens],
      ['合计', u.total_tokens],
    ]
      .filter(([, value]) => typeof value === 'number')
      .map(([name, value]) => `${name} ${value}`)
      .join(' · ') + ' tokens'
  )
}
function select(event: MouseEvent | KeyboardEvent, message: Message) {
  const selection = window.getSelection(),
    element = event.currentTarget as HTMLElement
  if (
    !selection ||
    !element.contains(selection.anchorNode) ||
    !element.contains(selection.focusNode)
  )
    return
  const text = selection.toString().trim()
  if (text)
    props.w.selected = {
      text: Array.from(text).slice(0, 5000).join(''),
      conversationId: c.value.id,
      messageId: message.id,
    }
}
function passage(message: Message) {
  const s = props.w.selected
  return s?.conversationId === c.value.id && s.messageId === message.id ? s.text : message.text
}
function collect(message: Message) {
  emit('collect', {
    id: crypto.randomUUID(),
    text: passage(message),
    language: c.value.target,
    meaning: '',
    note: '',
    tags: [],
    source: {
      conversationId: c.value.id,
      messageId: message.id,
      text: message.text,
      provider: profile.value?.provider ?? '',
      model: c.value.model,
    },
    createdAt: Date.now(),
    review: null,
  })
}
function explain(message: Message, task: 'explain' | 'translate') {
  emit('tutor')
  const text = passage(message)
  props.w.selected = { text, conversationId: c.value.id, messageId: message.id }
  void props.w.send(
    'tutor',
    task === 'translate' ? '请翻译这段内容。' : '请解释这段内容的词义和语法。',
    task,
    text,
  )
}
function remove() {
  if (confirm(`删除对话“${c.value.title}”？已收藏词句和原句快照仍会保留。`))
    props.w.removeConversation(props.pane)
}
function key(event: KeyboardEvent) {
  if (
    event.key === 'Enter' &&
    !event.shiftKey &&
    !event.isComposing &&
    !composing.value &&
    event.keyCode !== 229
  ) {
    event.preventDefault()
    void props.w.send(props.pane, undefined, mode.value)
  }
}
watch(
  () => [c.value.id, c.value.messages.length, c.value.messages.at(-1)?.text],
  async () => {
    const element = scroller.value
    const nearBottom =
      !element || element.scrollHeight - element.scrollTop - element.clientHeight < 160
    await nextTick()
    if (scroller.value && nearBottom) scroller.value.scrollTop = scroller.value.scrollHeight
  },
)
</script>
<template>
  <section class="web-pane" :aria-label="pane === 'main' ? '目标语言对话' : '语言助手'">
    <header class="pane-heading">
      <div>
        <span class="eyebrow">{{ pane === 'main' ? 'IMMERSE' : 'UNDERSTAND' }}</span>
        <h2>{{ pane === 'main' ? '目标语言对话' : '语言助手' }}</h2>
      </div>
      <button :disabled="!w.canEdit || w.busy[pane]" @click="w.fresh(pane)">＋ 新对话</button>
    </header>
    <div class="pane-config">
      <label
        >服务<select
          :aria-label="`${pane} 服务`"
          :value="c.profileId"
          :disabled="!w.canEdit || w.busy[pane]"
          @change="w.selectProfile(pane, ($event.target as HTMLSelectElement).value)"
        >
          <option value="" disabled>在设置中添加 API</option>
          <option v-for="p in w.state.profiles" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select></label
      >
      <label
        >模型<input
          :aria-label="`${pane} 模型`"
          :value="c.model"
          :list="`${pane}-models`"
          :disabled="!w.canEdit || w.busy[pane]"
          placeholder="输入模型 ID"
          maxlength="200"
          @change="w.selectModel(pane, ($event.target as HTMLInputElement).value)" /><datalist
          :id="`${pane}-models`"
        >
          <option v-for="model in choices" :key="model" :value="model" /></datalist
      ></label>
    </div>
    <label class="history-select"
      >历史<select
        :aria-label="`${pane} 历史`"
        :value="c.id"
        :disabled="!w.canEdit || w.busy[pane]"
        @change="w.selectConversation(pane, ($event.target as HTMLSelectElement).value)"
      >
        <option
          v-for="item in [...w.state.conversations].reverse().filter((item) => item.pane === pane)"
          :key="item.id"
          :value="item.id"
        >
          {{ item.title }}
        </option></select
      ><button
        :aria-label="`${pane} 删除对话`"
        :disabled="!w.canEdit || w.busy[pane]"
        @click="remove"
      >
        删除</button
      ><span>{{ c.target }}{{ pane === 'tutor' ? ` → ${c.native}` : '' }}</span></label
    >
    <div v-if="pane === 'tutor' && w.selected" class="quote-context">
      <strong>正在学习</strong>
      <p>{{ w.selected.text }}</p>
      <button aria-label="清除引用" @click="w.selected = null">×</button>
    </div>
    <div ref="scroller" class="web-messages" role="log" aria-live="polite">
      <div v-if="!c.messages.length" class="empty-chat">
        <span>{{ pane === 'main' ? 'Aa' : '文' }}</span>
        <h3>{{ pane === 'main' ? '从一句话开始。' : '把不懂的地方弄明白。' }}</h3>
        <p>
          {{
            pane === 'main'
              ? `用 ${c.target} 聊聊今天。遇到生词，选中后交给旁边的助手。`
              : '选中对话中的词句，或用母语提问。解释、翻译和表达练习都在这里。'
          }}
        </p>
      </div>
      <article
        v-for="message in c.messages"
        :id="`web-message-${message.id}`"
        :key="message.id"
        class="web-message"
        :class="message.role"
      >
        <div class="message-label">
          {{ message.role === 'user' ? '你' : pane === 'main' ? '对话伙伴' : '语言助手'
          }}<span v-if="message.status !== 'complete'">
            ·
            {{
              { streaming: '正在回复', interrupted: '已停止', failed: '未完成' }[
                message.status as 'streaming' | 'interrupted' | 'failed'
              ]
            }}</span
          >
        </div>
        <p class="message-text" @mouseup="select($event, message)" @keyup="select($event, message)">
          {{ message.text || (message.status === 'streaming' ? '…' : '') }}
        </p>
        <p v-if="message.error" class="web-error">{{ message.error }}</p>
        <small v-if="message.usage">{{ usageText(message) }}</small>
        <div v-if="message.text" class="message-actions">
          <button :disabled="!w.canEdit" @click="collect(message)">
            收藏{{ w.selected?.messageId === message.id ? '选段' : '整句' }}</button
          ><button
            v-if="pane === 'main'"
            :disabled="!w.ready('tutor')"
            @click="explain(message, 'explain')"
          >
            解释</button
          ><button
            v-if="pane === 'main'"
            :disabled="!w.ready('tutor')"
            @click="explain(message, 'translate')"
          >
            翻译
          </button>
        </div>
      </article>
    </div>
    <form class="web-composer" @submit.prevent="w.send(pane, undefined, mode)">
      <label v-if="pane === 'tutor'" class="tutor-mode"
        >任务<select v-model="mode" aria-label="辅导任务">
          <option value="express">怎么说</option>
          <option value="explain">解释</option>
          <option value="translate">翻译</option>
        </select></label
      >
      <textarea
        v-model="c.draft"
        :aria-label="`${pane} 输入`"
        :disabled="!w.canEdit"
        :placeholder="pane === 'main' ? `用 ${c.target} 写下你的想法…` : tutorPlaceholders[mode]"
        maxlength="20000"
        @compositionstart="composing = true"
        @compositionend="composing = false"
        @keydown="key"
      />
      <footer>
        <small>{{
          !profile
            ? '先在设置中添加 API'
            : !w.keys.get(profile.id)
              ? '请在设置中填写本次会话的密钥'
              : 'Enter 发送 · Shift+Enter 换行'
        }}</small
        ><button v-if="w.busy[pane]" type="button" class="primary" @click="w.stop(pane)">
          停止</button
        ><button v-else class="primary" :disabled="!w.ready(pane) || !c.draft.trim()">
          发送 ↑
        </button>
      </footer>
    </form>
  </section>
</template>
