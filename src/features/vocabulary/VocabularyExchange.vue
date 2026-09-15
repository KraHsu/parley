<script setup lang="ts">
import { ref } from 'vue'
import { wordInvoke as invoke } from '../../shared/word-operations'
import { useVocabularyStore } from './store'
interface Preview {
  token: string
  entries: number
  added: number
  duplicates: number
  conflicts: number
  conflictExamples: string[]
}
const vocabulary = useVocabularyStore()
const busy = ref(false)
const error = ref('')
const notice = ref('')
const includeTrash = ref(false)
const preview = ref<Preview | null>(null)
const copyConflicts = ref(false)
let requestId = ''
async function exportFile(format: 'json' | 'csv') {
  if (busy.value || !(await vocabulary.flush())) return
  busy.value = true
  error.value = ''
  notice.value = ''
  try {
    const path = await invoke<string | null>('vocabulary_export', {
      format,
      includeTrash: includeTrash.value,
    })
    if (path) notice.value = `已导出到 ${path}`
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
async function chooseImport() {
  if (busy.value) return
  busy.value = true
  error.value = ''
  notice.value = ''
  try {
    preview.value = await invoke<Preview | null>('vocabulary_preview_import')
    requestId = crypto.randomUUID()
    copyConflicts.value = false
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
async function importFile() {
  if (!preview.value || busy.value || !(await vocabulary.flush())) return
  busy.value = true
  error.value = ''
  try {
    const result = await invoke<{ added: number; duplicates: number; skipped: number }>(
      'vocabulary_import',
      { token: preview.value.token, requestId, copyConflicts: copyConflicts.value },
    )
    notice.value = `导入完成：新增 ${result.added} 条，重复 ${result.duplicates} 条，保留本地冲突 ${result.skipped} 条。`
    preview.value = null
    await vocabulary.refresh()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
function policyChanged() {
  requestId = crypto.randomUUID()
}
</script>
<template>
  <details class="word-exchange">
    <summary>导入与导出</summary>
    <p class="settings-help">
      JSON 可在桌面与 Web 之间迁移词句、原句、标签和复习记录；CSV
      适合表格查看，不用于完整恢复。编辑草稿不包含在导出中。
    </p>
    <label class="word-checkbox"
      ><input v-model="includeTrash" type="checkbox" :disabled="busy" />导出时包含回收站</label
    >
    <div class="word-exchange-actions">
      <button
        class="secondary-button"
        :disabled="busy || !vocabulary.initialized"
        @click="exportFile('json')"
      >
        导出 JSON</button
      ><button
        class="secondary-button"
        :disabled="busy || !vocabulary.initialized"
        @click="exportFile('csv')"
      >
        导出 CSV</button
      ><button
        class="secondary-button"
        :disabled="busy || !vocabulary.initialized"
        @click="chooseImport"
      >
        导入 JSON…
      </button>
    </div>
    <p v-if="busy" class="settings-help" role="status">正在处理数据，请完成文件选择或稍候…</p>
    <section v-if="preview" class="word-import-preview">
      <strong>导入预览 · {{ preview.entries }} 条词句</strong>
      <p>新增 {{ preview.added }} · 重复 {{ preview.duplicates }} · 冲突 {{ preview.conflicts }}</p>
      <p v-if="preview.conflictExamples.length" class="settings-help">
        冲突示例：{{ preview.conflictExamples.join('、') }}
      </p>
      <label v-if="preview.conflicts" class="word-checkbox"
        ><input
          v-model="copyConflicts"
          type="checkbox"
          :disabled="busy"
          @change="policyChanged"
        />将冲突记录作为另一条导入（默认保留本地）</label
      >
      <div class="word-exchange-actions">
        <button class="primary-button" :disabled="busy" @click="importFile">确认导入</button
        ><button class="text-button" :disabled="busy" @click="preview = null">取消</button>
      </div>
    </section>
    <p v-if="notice" class="settings-help" role="status">{{ notice }}</p>
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
  </details>
</template>
