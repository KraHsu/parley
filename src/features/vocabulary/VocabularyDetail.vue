<script setup lang="ts">
import { ref, watch } from 'vue'
import { useChatStore } from '../chat/store'
import { useVocabularyStore } from './store'
import { sourceLabel } from './sourceLabel'
import LearningControls from './LearningControls.vue'
const vocabulary = useVocabularyStore()
const chat = useChatStore()
const confirmPurge = ref(false)
watch(
  () => vocabulary.selected?.id,
  () => (confirmPurge.value = false),
)
</script>
<template>
  <section v-if="vocabulary.selected" class="word-detail" aria-label="词句详情">
    <header class="word-detail-header">
      <span class="overline">{{
        vocabulary.selected.languageLabel || vocabulary.selected.language
      }}</span
      ><button class="text-button" @click="vocabulary.selected = null">返回列表</button>
    </header>
    <h2 dir="auto">{{ vocabulary.selected.text }}</h2>
    <p dir="auto" class="word-meaning">
      {{ vocabulary.selected.meaning || '还没有释义，可以先记下自己的理解。' }}
    </p>
    <section v-if="vocabulary.selected.note">
      <h3>自己的注释</h3>
      <p dir="auto" class="word-note">{{ vocabulary.selected.note }}</p>
    </section>
    <section v-if="vocabulary.selected.occurrences.length">
      <h3>遇见它的地方</h3>
      <article
        v-for="source in vocabulary.selected.occurrences"
        :key="source.id"
        class="word-source"
      >
        <span class="overline">{{ sourceLabel(source) }}</span>
        <blockquote dir="auto">{{ source.snapshot }}</blockquote>
        <small
          >{{ source.truncated ? '来源片段 · ' : ''
          }}{{
            source.conversationId || source.threadId ? '已保留原句快照' : '来源未关联，原句仍可查看'
          }}</small
        >
        <button
          v-if="source.conversationId && source.messageId"
          class="text-button"
          :disabled="chat.navigating || chat.closing"
          @click="chat.selectConversation(source.conversationId, source.messageId)"
        >
          查看原消息
        </button>
      </article>
    </section>
    <p v-else class="settings-help">手动添加的词句，没有来源原句。</p>
    <LearningControls />
    <div class="word-detail-actions">
      <template v-if="!vocabulary.selected.deletedAt"
        ><button
          class="primary-button"
          :disabled="vocabulary.busy"
          @click="vocabulary.edit(vocabulary.selected)"
        >
          编辑词句</button
        ><button
          class="text-button"
          :disabled="vocabulary.busy"
          @click="vocabulary.mutate(vocabulary.selected, 'trash')"
        >
          移入回收站
        </button></template
      >
      <template v-else
        ><button
          class="primary-button"
          :disabled="vocabulary.busy"
          @click="vocabulary.mutate(vocabulary.selected, 'restore')"
        >
          恢复词句</button
        ><button v-if="!confirmPurge" class="text-button danger-text" @click="confirmPurge = true">
          彻底删除…</button
        ><button
          v-else
          class="text-button danger-text"
          :disabled="vocabulary.busy"
          @click="vocabulary.mutate(vocabulary.selected, 'purge')"
        >
          确认删除词句及所有学习记录
        </button></template
      >
    </div>
  </section>
</template>
