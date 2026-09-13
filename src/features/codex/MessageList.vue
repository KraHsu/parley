<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import type { Message } from './store'
const props = defineProps<{ messages: Message[]; busy: boolean }>()
const root = ref<HTMLElement>()
watch(
  () => [props.messages.length, props.messages.at(-1)?.text, props.busy],
  async () => {
    const scroller = root.value?.closest('.scroll-region')
    if (!scroller) return
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
      :class="`message-${message.role}`"
    >
      <span class="message-author">{{
        message.role === 'user' ? '你' : message.role === 'assistant' ? 'GPT' : '新会话'
      }}</span>
      <p dir="auto">{{ message.text }}</p>
      <small
        v-if="message.status === 'interrupted' || message.status === 'failed'"
        class="message-state"
        >{{ message.status === 'interrupted' ? '已中断 · 内容可能不完整' : '请求未完成' }}</small
      >
    </article>
    <p v-if="busy" class="stream-status" role="status"><span class="tiny-dot" />正在回复…</p>
  </div>
</template>
