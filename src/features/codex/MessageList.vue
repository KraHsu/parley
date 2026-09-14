<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { formatUsage } from '../chat/usage'
import StudyText from '../vocabulary/StudyText.vue'
import { useChatStore } from '../chat/store'
import { useSettingsStore } from '../settings/store'
import type { Message } from '../chat/store'
const props = defineProps<{ messages: Message[]; busy: boolean; pane: 'main' | 'tutor' }>()
const codex = useChatStore()
const settings = useSettingsStore()
const conversation = computed(() => codex.history.find((c) => c.id === codex.lanes[props.pane].id))
const root = ref<HTMLElement>()
const focusedMessage = computed(() => {
  const focus = codex.sourceFocus
  return focus?.pane === props.pane && focus.conversationId === codex.lanes[props.pane].id
    ? focus.messageId
    : null
})
watch(
  () => [codex.sourceFocus?.request, codex.lanes[props.pane].id],
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
          conversationId: codex.lanes[pane].id,
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
          (message.usage || (!busy && codex.lanes[pane].backend.kind !== 'codex'))
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
    <p v-if="codex.lanes[pane].notice" class="settings-help" role="status">
      {{ codex.lanes[pane].notice }}
    </p>
    <p v-if="busy" class="stream-status" role="status"><span class="tiny-dot" />正在回复…</p>
  </div>
</template>
