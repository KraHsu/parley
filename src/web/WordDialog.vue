<script setup lang="ts">
import { ref } from 'vue'
import type { WebWorkspace } from './store'
import type { Word } from './types'
const props = defineProps<{ w: WebWorkspace }>()
const dialog = ref<HTMLDialogElement>(),
  word = ref<Word>(),
  tags = ref(''),
  error = ref('')
function show(value: Word) {
  word.value = JSON.parse(JSON.stringify(value))
  tags.value = value.tags.join(', ')
  error.value = ''
  dialog.value?.showModal()
}
function save() {
  if (!word.value) return
  try {
    const value = JSON.parse(JSON.stringify(word.value)) as Word
    value.tags = [
      ...new Set(
        tags.value
          .split(/[,，]/)
          .map((s) => s.trim())
          .filter(Boolean),
      ),
    ].slice(0, 30)
    const duplicate = props.w.state.words.find(
      (w) =>
        w.id !== value.id &&
        w.language === value.language &&
        w.text.trim().normalize('NFKC') === value.text.trim().normalize('NFKC'),
    )
    if (duplicate) throw new Error('已有相同词句，请在词句列表中编辑原记录。')
    props.w.saveWord(value)
    props.w.notice = '词句已保存。'
    dialog.value?.close()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
function remove() {
  if (!word.value || !confirm(`删除“${word.value.text}”及其复习记录？`)) return
  props.w.state.words = props.w.state.words.filter((w) => w.id !== word.value!.id)
  dialog.value?.close()
}
defineExpose({ show })
</script>
<template>
  <dialog ref="dialog" class="web-dialog word-dialog">
    <header>
      <div>
        <span class="eyebrow">KEEP THE CONTEXT</span>
        <h2>收藏词句</h2>
      </div>
      <button aria-label="关闭词句编辑" @click="dialog?.close()">×</button>
    </header>
    <form v-if="word" @submit.prevent="save">
      <div class="dialog-body">
        <label>词句<textarea v-model="word.text" required maxlength="10000" /></label
        ><label>语言<input v-model="word.language" required maxlength="100" /></label>
        <blockquote v-if="word.source">{{ word.source.text }}</blockquote>
        <label
          >释义<textarea
            v-model="word.meaning"
            maxlength="10000"
            placeholder="用自己的话记住它的意思"
          /></label
        ><label
          >我的注释<textarea
            v-model="word.note"
            maxlength="10000"
            placeholder="搭配、语法或另一个例句"
          /></label
        ><label
          >标签<input v-model="tags" maxlength="3000" placeholder="用逗号分隔，例如 daily, grammar"
        /></label>
        <p v-if="error" class="web-error" role="alert">{{ error }}</p>
      </div>
      <footer>
        <button
          v-if="w.state.words.some((w) => w.id === word?.id)"
          type="button"
          :disabled="!w.canEdit"
          @click="remove"
        >
          删除词句</button
        ><button class="primary" :disabled="!w.canEdit">保存词句</button>
      </footer>
    </form>
  </dialog>
</template>
