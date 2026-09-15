<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { formatUsage } from '../chat/usage'
import StudyText from '../vocabulary/StudyText.vue'
import { useChatStore } from './store'
import { useSettingsStore } from '../settings/store'
import type { Message } from './store'
const props = defineProps<{ messages: Message[]; busy: boolean; pane: 'main' | 'tutor' }>()
const chat = useChatStore()
const settings = useSettingsStore()
const conversation = computed(() => chat.history.find((c) => c.id === chat.lanes[props.pane].id))
const root = ref<HTMLElement>()
const focusedMessage = computed(() => {
  const focus = chat.sourceFocus
  return focus?.pane === props.pane && focus.conversationId === chat.lanes[props.pane].id
    ? focus.messageId
    : null
})
watch(
  () => [chat.sourceFocus?.request, chat.lanes[props.pane].id],
  async () => {
    const messageId = focusedMessage.value
    if (!messageId) return
    await nextTick()
    if (focusedMessage.value !== messageId) return
    const article = Array.from(
      root.value?.querySelectorAll<HTMLElement>('[data-message-id]') ?? [],
    ).find((element) => element.dataset.messageId === messageId)
    article?.scrollIntoView({ block: 'center' })
    article?.focus({ preventScroll: true })
  },
  { immediate: true, flush: 'post' },
)

watch(
  () => [props.messages.length, props.messages.at(-1)?.text, props.busy],
  async () => {
    const scroller = root.value?.closest('.scroll-region')
    if (!scroller) return
    if (window.getSelection()?.toString() || focusedMessage.value) return
    const nearBottom = scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 120
    await nextTick()
    if (nearBottom) scroller.scrollTop = scroller.scrollHeight
  },
)
</script>
<template>
  <div ref="root" class="message-list" role="log" aria-label="消息记录" aria-live="off">
    <article
      v-for="message in messages"
      :key="message.id"
      class="chat-message"
      :class="[`message-${message.role}`, { 'source-message': message.id === focusedMessage }]"
      :data-message-id="message.id"
      tabindex="-1"
    >
      <span class="message-author">{{
        message.role === 'user' ? '你' : message.role === 'assistant' ? '助手' : '新会话'
      }}</span>
      <StudyText
        v-if="message.role !== 'note'"
        :text="message.text"
        :language="conversation?.targetLanguage ?? settings.targetLanguage"
        :allow-answer="pane === 'tutor' && message.role === 'assistant'"
        :answer-target="message.vocabularyTarget"
        :origin="{
          sourceKind: pane,
          conversationId: chat.lanes[pane].id,
          messageId: message.id,
          threadId: conversation?.threadId ?? null,
          turnId: null,
          itemId: message.id,
          role: message.role,
          truncated: false,
        }"
      />
      <p v-else dir="auto">{{ message.text }}</p>
      <small
        v-if="
          message.role === 'assistant' &&
          (message.usage || (!busy && chat.lanes[pane].backend.kind !== 'codex'))
        "
        class="message-usage"
        aria-label="服务返回的 token 用量"
        >{{ formatUsage(message.usage) || '服务未提供用量' }}</small
      >
      <small
        v-if="message.status === 'interrupted' || message.status === 'failed'"
        class="message-state"
        >{{ message.status === 'interrupted' ? '已中断 · 内容可能不完整' : '请求未完成' }}</small
      >
    </article>
    <p v-if="chat.lanes[pane].notice" class="settings-help" role="status">
      {{ chat.lanes[pane].notice }}
    </p>
    <p v-if="busy" class="stream-status" role="status"><span class="tiny-dot" />正在回复…</p>
  </div>
</template>
