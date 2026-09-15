<script setup lang="ts">
import { ref } from 'vue'
import { readLearningBackup } from '../shared/learning-exchange'
import { previewLearning, type LearningPreview } from './learning-exchange'
import type { WebWorkspace } from './store'
const props = defineProps<{ w: WebWorkspace }>()
const file = ref<HTMLInputElement>(),
  dialog = ref<HTMLDialogElement>()
const preview = ref<LearningPreview>(),
  busy = ref(false),
  error = ref(''),
  copies = ref(false)
async function choose(event: Event) {
  const input = event.target as HTMLInputElement,
    selected = input.files?.[0]
  input.value = ''
  if (!selected) return
  busy.value = true
  error.value = ''
  try {
    if (!(await props.w.persist())) throw new Error(props.w.storageError)
    preview.value = await previewLearning(await readLearningBackup(selected), props.w.state.words)
    copies.value = false
    dialog.value?.showModal()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
async function apply() {
  if (!preview.value || busy.value) return
  busy.value = true
  try {
    if (await props.w.importLearning(preview.value, copies.value)) {
      dialog.value?.close()
      preview.value = undefined
    }
  } finally {
    busy.value = false
  }
}
async function download() {
  busy.value = true
  try {
    await props.w.downloadLearning()
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <details class="word-exchange">
    <summary>词句迁移 · Web ↔ 桌面</summary>
    <p class="help">
      词句 JSON 可在 Web 和桌面之间导入，保留原文、标签、回收站和复习记录。导入会合并词句；对话和
      API 设置不受影响。
    </p>
    <div class="message-actions">
      <button :disabled="busy || !w.canEdit || w.busy.main || w.busy.tutor" @click="download">
        导出词句 JSON
      </button>
      <button :disabled="busy || !w.canEdit || w.busy.main || w.busy.tutor" @click="file?.click()">
        导入词句 JSON…
      </button>
    </div>
    <p v-if="busy" class="help" role="status">正在处理词句数据…</p>
    <p v-if="error" class="web-error" role="alert">{{ error }}</p>
    <input
      ref="file"
      type="file"
      aria-label="导入词句 JSON 文件"
      accept=".json,application/json"
      hidden
      @change="choose"
    />
    <dialog
      ref="dialog"
      class="web-dialog"
      aria-labelledby="learning-import-title"
      @cancel="busy && $event.preventDefault()"
    >
      <header>
        <h2 id="learning-import-title">导入词句预览</h2>
        <button :disabled="busy" aria-label="取消词句导入" @click="dialog?.close()">×</button>
      </header>
      <div v-if="preview" class="dialog-body">
        <p>
          新增 {{ preview.added }} · 重复 {{ preview.duplicates }} · 冲突 {{ preview.conflicts }}
        </p>
        <p class="help">
          包含 {{ preview.trashed }} 条回收站记录。重复内容跳过，冲突默认保留本地记录。
        </p>
        <label v-if="preview.conflicts"
          ><input
            v-model="copies"
            type="checkbox"
            :disabled="busy"
          />将冲突记录作为另一条导入</label
        >
        <p v-if="w.error" class="web-error" role="alert">{{ w.error }}</p>
      </div>
      <footer>
        <button :disabled="busy" @click="dialog?.close()">取消</button
        ><button class="primary" :disabled="busy || !w.canEdit" @click="apply">确认导入词句</button>
      </footer>
    </dialog>
  </details>
</template>
