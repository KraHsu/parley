<script setup lang="ts">
import type { Conversation } from './types'
defineProps<{ messages: NonNullable<Conversation['context']> }>()
</script>

<template>
  <details v-if="messages.length" class="imported-context">
    <summary>带入的历史 · {{ messages.length }} 条</summary>
    <p class="settings-help">创建本会话时带入的文字快照，不会同步原会话的后续改动。</p>
    <div class="imported-context-messages">
      <article v-for="(message, index) in messages" :key="index">
        <small
          >{{ message.role === 'user' ? '你' : '助手'
          }}<template v-if="message.status === 'interrupted'"> · 已中断</template
          ><template v-else-if="message.status === 'failed'"> · 未完成</template></small
        >
        <p dir="auto">{{ message.text }}</p>
      </article>
    </div>
  </details>
</template>

<style scoped>
.imported-context {
  margin: 20px;
  padding: 16px;
  border: 1px solid var(--line);
  border-radius: 12px;
}
summary {
  cursor: pointer;
  font-size: 13px;
}
.imported-context-messages {
  max-height: 320px;
  overflow: auto;
}
article + article {
  margin-top: 16px;
}
small {
  opacity: 0.7;
}
article p {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  font-size: 14px;
  line-height: 1.6;
}
</style>
