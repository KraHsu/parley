<script setup lang="ts">
import { ref } from 'vue'
import { parseTags } from '../shared/learning-fields'
import type { WebWorkspace } from './store'
import type { Word } from './types'
const props = defineProps<{ w: WebWorkspace }>()
const dialog = ref<HTMLDialogElement>(),
  word = ref<Word>(),
  tags = ref(''),
  error = ref(''),
  saving = ref(false)
function show(value: Word) {
  word.value = JSON.parse(JSON.stringify(value))
  tags.value = value.tags.join(', ')
  error.value = ''
  dialog.value?.showModal()
}
async function save() {
  if (!word.value || saving.value) return
  saving.value = true
  error.value = ''
  try {
    const value = JSON.parse(JSON.stringify(word.value)) as Word
    value.tags = parseTags(tags.value)
    await props.w.saveWord(value)
    props.w.notice = '词句已保存。'
    dialog.value?.close()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    saving.value = false
  }
}
async function remove() {
  if (!word.value || saving.value || !confirm(`删除“${word.value.text}”及其复习记录？`)) return
  saving.value = true
  try {
    await props.w.removeWord(word.value.id)
    dialog.value?.close()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    saving.value = false
  }
}
defineExpose({ show })
</script>
<template>
  <dialog
    ref="dialog"
    class="web-dialog word-dialog"
    aria-labelledby="web-word-title"
    @cancel="saving && $event.preventDefault()"
  >
    <header>
      <div>
        <span class="eyebrow">KEEP THE CONTEXT</span>
        <h2 id="web-word-title">收藏词句</h2>
      </div>
      <button :disabled="saving" aria-label="关闭词句编辑" @click="dialog?.close()">×</button>
    </header>
    <form :inert="saving" v-if="word" @submit.prevent="save">
      <div class="dialog-body">
        <label>词句<textarea v-model="word.text" required maxlength="10000" /></label
        ><label>语言<input v-model="word.language" required maxlength="100" /></label>
        <blockquote v-if="word.source">{{ word.source.text }}</blockquote>
        <details v-if="word.learning" class="help">
          <summary>
            迁移记录：{{ word.learning.entry.occurrences.length }} 个来源 ·
            {{ word.learning.entry.cards.length }} 张卡片 ·
            {{ word.learning.entry.reviews.length }} 次复习
          </summary>
          <p>Web 可以编辑词句并进行识义复习；其他来源、表达卡片和历史也会保留在导出文件中。</p>
          <blockquote v-for="source in word.learning.entry.occurrences.slice(1)" :key="source.id">
            {{ source.snapshot }}
          </blockquote>
        </details>
        <label
          >释义<textarea
            v-model="word.meaning"
            maxlength="10000"
            placeholder="用自己的话记住它的意思"
          /></label
        ><label
          >我的注释<textarea
            v-model="word.note"
            maxlength="16000"
            placeholder="搭配、语法或另一个例句"
          /></label
        ><label
          >标签<input v-model="tags" maxlength="3000" placeholder="用逗号分隔，例如 daily, grammar"
        /></label>
        <p v-if="error" class="web-error" role="alert">{{ error }}</p>
        <button v-if="w.storageError" type="button" @click="w.persist()">重试保存</button>
      </div>
      <footer>
        <button
          v-if="w.state.words.some((w) => w.id === word?.id)"
          type="button"
          :disabled="!w.canEdit || saving"
          @click="remove"
        >
          删除词句</button
        ><button class="primary" :disabled="!w.canEdit || saving">保存词句</button>
      </footer>
    </form>
  </dialog>
</template>
