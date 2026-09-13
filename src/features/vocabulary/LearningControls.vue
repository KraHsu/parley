<script setup lang="ts">
import { ref, watch } from 'vue'
import { wordInvoke as invoke } from '../../shared/word-operations'
import { useVocabularyStore } from './store'
import type { ReviewCard } from './types'
const vocabulary = useVocabularyStore()
const busy = ref(false)
const error = ref('')
const resetConfirm = ref('')
const retries = new Map<string, string>()
watch(
  () => vocabulary.selected,
  () => {
    error.value = ''
    resetConfirm.value = ''
  },
  { immediate: true },
)

async function card(direction: string, existing?: ReviewCard, reset = false) {
  const entry = vocabulary.selected
  if (!entry || busy.value) return
  busy.value = true
  error.value = ''
  const key = JSON.stringify([entry.id, direction, existing?.revision, reset])
  if (!retries.has(key)) retries.set(key, crypto.randomUUID())
  try {
    await invoke('vocabulary_card_save', {
      request: {
        requestId: retries.get(key)!,
        entryId: entry.id,
        direction,
        suspended: reset ? false : existing ? !existing.suspended : false,
        reset,
        expectedRevision: existing?.revision ?? null,
      },
    })
    if (vocabulary.selected?.id === entry.id) await vocabulary.open(entry.id)
    await vocabulary.refresh()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <section v-if="vocabulary.selected && !vocabulary.selected.deletedAt" class="learning-controls">
    <h3>标签</h3>
    <div class="word-tag-form">
      <span>{{ vocabulary.selected.tags.join(' · ') || '还没有标签' }}</span
      ><button class="text-button" @click="vocabulary.edit(vocabulary.selected)">编辑标签</button>
    </div>
    <h3>复习卡片</h3>
    <p v-if="!vocabulary.selected.meaning.trim()" class="settings-help">
      先补充释义，才能加入复习。
    </p>
    <div
      v-for="direction in ['recognition', 'production'] as const"
      :key="direction"
      class="learning-card-row"
    >
      <div>
        <strong>{{
          direction === 'recognition' ? '看到词句，回忆含义' : '看到释义，回忆表达'
        }}</strong
        ><small v-if="vocabulary.selected.cards.find((c) => c.direction === direction)">{{
          vocabulary.selected.cards.find((c) => c.direction === direction)?.suspended
            ? '已暂停'
            : `到期：${new Date(vocabulary.selected.cards.find((c) => c.direction === direction)!.dueAt).toLocaleString()}`
        }}</small>
      </div>
      <button
        class="secondary-button"
        :disabled="busy || !vocabulary.selected.meaning.trim()"
        @click="
          card(
            direction,
            vocabulary.selected.cards.find((c) => c.direction === direction),
          )
        "
      >
        {{
          vocabulary.selected.cards.some((c) => c.direction === direction)
            ? vocabulary.selected.cards.find((c) => c.direction === direction)?.suspended
              ? '继续复习'
              : '暂停'
            : '加入复习'
        }}
      </button>
      <button
        v-if="vocabulary.selected.cards.some((c) => c.direction === direction)"
        class="text-button"
        :disabled="busy"
        @click="
          resetConfirm === direction
            ? card(
                direction,
                vocabulary.selected.cards.find((c) => c.direction === direction),
                true,
              )
            : (resetConfirm = direction)
        "
      >
        {{ resetConfirm === direction ? '确认重置' : '重置…' }}
      </button>
    </div>
    <p class="settings-help">修改释义会保留排程；需要重新学习时可重置对应卡片。</p>
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
  </section>
</template>
