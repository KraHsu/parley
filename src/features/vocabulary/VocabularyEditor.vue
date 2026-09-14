<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { wordInvoke as invoke } from '../../shared/word-operations'
import { useVocabularyStore } from './store'
import { useChatStore } from '../chat/store'
import type { EntryPage, VocabularyEntry } from './types'
const vocabulary = useVocabularyStore()
const codex = useChatStore()
const dialog = ref<HTMLDialogElement>()
const discardConfirm = ref(false)
const related = ref<EntryPage['entries']>([])
let generation = 0
watch(
  () => vocabulary.editorOpen,
  async (opened) => {
    await nextTick()
    if (opened && !dialog.value?.open) dialog.value?.showModal()
    else if (!opened) dialog.value?.close()
    discardConfirm.value = false
  },
)
watch(
  () => [vocabulary.editor?.fields.text, vocabulary.editor?.fields.language],
  async () => {
    const current = ++generation
    const text = vocabulary.editor?.fields.text.trim()
    related.value = []
    if (!text || !vocabulary.editor?.occurrence) return
    try {
      const page = await invoke<EntryPage>('vocabulary_list', {
        query: { search: text, language: vocabulary.editor.fields.language },
      })
      if (current === generation)
        related.value = page.entries.filter((e) => e.id !== vocabulary.editor?.entryId)
    } catch {
      /* The normal list provides the retryable storage error. */
    }
  },
)
async function viewDuplicate() {
  if (vocabulary.duplicate) await vocabulary.open(vocabulary.duplicate.id)
  await vocabulary.closeEditor()
  vocabulary.activeTab = 'words'
  vocabulary.wordFocusRequest++
}
async function saveAnotherMeaning() {
  if (vocabulary.editor) vocabulary.editor.allowDuplicate = true
  await vocabulary.save()
}
async function explain() {
  const draft = vocabulary.editor
  if (!draft || !(await vocabulary.flushDraft())) return
  const answerTarget = { id: draft.id, requestId: draft.requestId }
  let sentence = draft.occurrence?.snapshot ?? ''
  if (!sentence && draft.entryId) {
    try {
      const entry = await invoke<VocabularyEntry>('vocabulary_get', { id: draft.entryId })
      sentence = entry.occurrences[0]?.snapshot ?? ''
    } catch (error) {
      vocabulary.error = String(error)
      return
    }
  }
  const snapshot = JSON.stringify({
    word: draft.fields.text,
    language: draft.fields.language,
    sentence,
  })
  await vocabulary.closeEditor()
  codex.tutorMode = 'explain'
  vocabulary.tutorFocusRequest++
  codex.mobilePane = 'tutor'
  await codex.send(
    'tutor',
    `请解释下面的学习词句。先给简短释义，再解释用法。\n${snapshot}`,
    'explain',
    answerTarget,
  )
}
</script>
<template>
  <dialog
    ref="dialog"
    class="vocabulary-editor"
    aria-labelledby="word-editor-title"
    @cancel.prevent="vocabulary.closeEditor()"
  >
    <form v-if="vocabulary.editor" :inert="codex.closing" @submit.prevent="vocabulary.save()">
      <header class="dialog-heading">
        <div>
          <p class="overline">KEEP THE CONTEXT</p>
          <h2 id="word-editor-title">{{ vocabulary.editor.entryId ? '编辑词句' : '收藏词句' }}</h2>
        </div>
        <button
          type="button"
          class="icon-button"
          aria-label="保存草稿并收起"
          :disabled="vocabulary.busy"
          @click="vocabulary.closeEditor()"
        >
          ×
        </button>
      </header>
      <div class="word-editor-scroll scroll-region">
        <fieldset :disabled="vocabulary.busy">
          <label
            >词句<textarea
              v-model="vocabulary.editor.fields.text"
              dir="auto"
              rows="2"
              required
              placeholder="想记住的词、短语或句子"
            />
          </label>
          <div class="word-form-row">
            <label
              >语言代码<input
                v-model="vocabulary.editor.fields.language"
                list="word-language-codes"
                required
                placeholder="en / ja / pt-BR"
            /></label>
            <label
              >类型<select v-model="vocabulary.editor.fields.kind">
                <option value="word">词</option>
                <option value="phrase">短语</option>
                <option value="sentence">句子</option>
              </select></label
            >
          </div>
          <datalist id="word-language-codes">
            <option value="en" />
            <option value="zh-CN" />
            <option value="ja" />
            <option value="ko" />
            <option value="fr" />
            <option value="de" />
            <option value="es" />
            <option value="ar" />
          </datalist>
          <label
            >语言名称（可选）<input
              v-model="vocabulary.editor.fields.languageLabel"
              placeholder="例如：Português"
          /></label>
          <section v-if="vocabulary.editor.occurrence" class="word-source">
            <p class="overline">
              原句 ·
              {{ vocabulary.editor.occurrence.sourceKind === 'terminal' ? '终端对话' : '对话记录' }}
            </p>
            <blockquote dir="auto">{{ vocabulary.editor.occurrence.snapshot }}</blockquote>
            <small v-if="vocabulary.editor.occurrence.truncated">保存的是来源片段。</small>
          </section>
          <label
            >释义<textarea
              v-model="vocabulary.editor.fields.meaning"
              dir="auto"
              rows="3"
              placeholder="可以先收藏，之后再补充自己的理解"
            />
          </label>
          <label
            >释义语言<input
              v-model="vocabulary.editor.fields.meaningLanguage"
              list="word-language-codes"
              required
          /></label>
          <label
            >自己的注释<textarea
              v-model="vocabulary.editor.fields.note"
              dir="auto"
              rows="3"
              placeholder="用法、容易混淆的表达，或自己的例句…"
            />
          </label>
          <label
            >标签（逗号分隔）<input
              v-model="vocabulary.editor.tagText"
              placeholder="例如：日常, 工作"
          /></label>
          <button
            type="button"
            class="secondary-button"
            :disabled="
              !codex.isReady('tutor') ||
              codex.lanes.tutor.busy ||
              !vocabulary.editor.fields.text.trim()
            "
            @click="explain"
          >
            请助手解释
          </button>
          <p class="settings-help">只在点击后请求模型。可在助手答案中选段，直接用作释义或注释。</p>
          <section v-if="!vocabulary.editor.entryId && related.length" class="word-related">
            <strong>已有相近词句</strong>
            <p class="settings-help">可以只补充这次原句，保留已有释义和注释。</p>
            <button
              v-for="entry in related.slice(0, 5)"
              :key="entry.id"
              type="button"
              class="word-related-item"
              @click="vocabulary.attach(entry)"
            >
              {{ entry.text }} · {{ entry.meaning || '尚无释义' }} <span>补充原句</span>
            </button>
          </section>
        </fieldset>
        <div v-if="vocabulary.duplicate" class="word-duplicate" role="status">
          <p>
            这处原句已收藏：{{ vocabulary.duplicate.text }} ·
            {{ vocabulary.duplicate.meaning || '尚无释义' }}
          </p>
          <button type="button" class="text-button" @click="viewDuplicate">查看原收藏</button>
          <button type="button" class="text-button" @click="saveAnotherMeaning">
            保存为另一种意思
          </button>
        </div>
        <section
          v-if="vocabulary.editor.entryId && vocabulary.error.includes('已有更新')"
          class="word-duplicate"
        >
          <button type="button" class="text-button" @click="vocabulary.compareLatest">
            查看最新版本并比较
          </button>
        </section>
        <section v-if="vocabulary.latestVersion" class="word-source">
          <h3>数据库中的最新版本</h3>
          <p dir="auto">{{ vocabulary.latestVersion.meaning }}</p>
          <p dir="auto">{{ vocabulary.latestVersion.note }}</p>
          <p class="settings-help">上方输入框保留你的草稿。请比较和修改后，再明确保存。</p>
          <button
            type="button"
            class="secondary-button"
            :disabled="vocabulary.busy"
            @click="vocabulary.acceptComparedDraft"
          >
            以当前草稿更新这个版本
          </button>
        </section>
        <p v-if="vocabulary.error || vocabulary.draftError" class="inline-error" role="alert">
          {{ vocabulary.error || vocabulary.draftError }}
        </p>
        <button
          v-if="vocabulary.draftError"
          type="button"
          class="text-button"
          @click="vocabulary.flushDraft()"
        >
          重试保存草稿
        </button>
      </div>
      <footer class="dialog-footer">
        <button
          v-if="!discardConfirm"
          type="button"
          class="text-button"
          :disabled="vocabulary.busy"
          @click="discardConfirm = true"
        >
          丢弃草稿
        </button>
        <button
          v-else
          type="button"
          class="text-button danger-text"
          :disabled="vocabulary.busy"
          @click="vocabulary.discard()"
        >
          确认丢弃
        </button>
        <button type="submit" class="primary-button" :disabled="vocabulary.busy">
          {{ vocabulary.busy ? '正在保存…' : '保存词句' }}
        </button>
      </footer>
    </form>
  </dialog>
</template>
