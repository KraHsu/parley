import { parseTags } from '../../shared/learning-fields'
import { defineStore } from 'pinia'
import { reactive, ref, watch } from 'vue'
import { wordInvoke as invoke } from '../../shared/word-operations'
import { isDesktop } from '../../shared/desktop'
import { useSettingsStore } from '../settings/store'
import type {
  EditDraft,
  EntryFields,
  EntryPage,
  ListQuery,
  SaveResult,
  VocabularyEntry,
  VocabularySource,
} from './types'

const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value))
export const useVocabularyStore = defineStore('vocabulary', () => {
  const settings = useSettingsStore()
  const initialized = ref(false)
  const loading = ref(false)
  const busy = ref(false)
  const error = ref('')
  const draftError = ref('')
  const notice = ref('')
  const tutorFocusRequest = ref(0)
  const wordFocusRequest = ref(0)
  const latestVersion = ref<VocabularyEntry | null>(null)
  const activeTab = ref('words')
  const entries = ref<VocabularyEntry[]>([])
  const total = ref(0)
  const activeCount = ref(0)
  const tags = ref<string[]>([])
  const languages = ref<string[]>([])
  const drafts = ref<EditDraft[]>([])
  const editor = ref<EditDraft | null>(null)
  const selected = ref<VocabularyEntry | null>(null)
  const duplicate = ref<VocabularyEntry | null>(null)
  const editorOpen = ref(false)
  const query = reactive<ListQuery>({
    search: '',
    language: '',
    kind: '',
    hasMeaning: null,
    trash: false,
    sort: 'updated',
    offset: 0,
    tag: '',
    reviewStatus: '',
  })
  const mutationIds = new Map<string, string>()
  function mutationId(value: unknown) {
    const key = JSON.stringify(value)
    let id = mutationIds.get(key)
    if (!id) {
      id = crypto.randomUUID()
      mutationIds.set(key, id)
    }
    return id
  }
  let readGeneration = 0
  let detailGeneration = 0
  let dirty = false
  let applying = false
  let saving: Promise<boolean> | undefined
  let operation: Promise<unknown> | undefined
  let timer: ReturnType<typeof setTimeout> | undefined
  let searchTimer: ReturnType<typeof setTimeout> | undefined

  function apply(draft: EditDraft | null) {
    applying = true
    editor.value = draft ? { ...clone(draft), tagText: draft.tagText ?? '' } : null
    applying = false
    duplicate.value = null
    latestVersion.value = null
  }
  watch(
    () => editor.value && [editor.value.fields, editor.value.allowDuplicate, editor.value.tagText],
    () => {
      if (applying || !editor.value) return
      editor.value.requestId = crypto.randomUUID()
      duplicate.value = null
      dirty = true
      clearTimeout(timer)
      timer = setTimeout(() => void flushDraft(), 250)
    },
    { deep: true, flush: 'sync' },
  )

  async function refreshDrafts() {
    drafts.value = await invoke<EditDraft[]>('vocabulary_load_drafts')
  }
  async function flushDraft(): Promise<boolean> {
    clearTimeout(timer)
    if (!initialized.value) return !dirty
    if (saving) {
      const ok = await saving
      return ok && dirty ? flushDraft() : ok
    }
    saving = (async () => {
      while (dirty && editor.value) {
        const snapshot = clone(editor.value)
        dirty = false
        try {
          await invoke('vocabulary_save_draft', { draft: snapshot })
          draftError.value = ''
          const index = drafts.value.findIndex((d) => d.id === snapshot.id)
          if (index < 0) drafts.value.unshift(snapshot)
          else drafts.value[index] = snapshot
        } catch (e) {
          dirty = true
          draftError.value = String(e)
          return false
        }
      }
      return true
    })()
    try {
      return await saving
    } finally {
      saving = undefined
    }
  }
  async function flush(): Promise<boolean> {
    if (operation) await operation
    return flushDraft()
  }
  async function refresh() {
    if (!initialized.value) return
    const generation = ++readGeneration
    loading.value = true
    try {
      const page = await invoke<EntryPage>('vocabulary_list', { query: clone(query) })
      if (generation !== readGeneration) return
      entries.value = page.entries
      total.value = page.total
      activeCount.value = page.activeCount
      tags.value = page.tags
      languages.value = page.languages
      error.value = ''
    } catch (e) {
      if (generation === readGeneration) error.value = String(e)
    } finally {
      if (generation === readGeneration) loading.value = false
    }
  }
  watch(
    () => [
      query.search,
      query.language,
      query.kind,
      query.hasMeaning,
      query.trash,
      query.sort,
      query.tag,
      query.reviewStatus,
    ],
    () => {
      query.offset = 0
      // Invalidate already-running requests immediately, before debounce elapses.
      readGeneration++
      clearTimeout(searchTimer)
      searchTimer = setTimeout(() => void refresh(), 180)
    },
  )
  async function page(offset: number) {
    query.offset = Math.max(0, offset)
    await refresh()
  }
  async function initialize() {
    if (initialized.value || !isDesktop()) return
    try {
      await refreshDrafts()
      initialized.value = true
      await refresh()
    } catch (e) {
      error.value = String(e)
    }
  }
  async function start(source: VocabularySource | null = null, language = settings.targetLanguage) {
    if (!initialized.value || busy.value || !(await flushDraft())) return
    apply({
      id: crypto.randomUUID(),
      entryId: null,
      baseRevision: null,
      fields: {
        language,
        languageLabel: '',
        kind: source ? 'phrase' : 'word',
        text: source?.selectedText ?? '',
        meaning: '',
        meaningLanguage: settings.nativeLanguage,
        note: '',
      },
      occurrence: source ? clone(source) : null,
      allowDuplicate: false,
      tagText: '',
      requestId: crypto.randomUUID(),
    })
    editorOpen.value = true
    dirty = true
    notice.value = ''
    error.value = ''
    await flushDraft()
  }
  async function open(id: string) {
    const generation = ++detailGeneration
    try {
      const entry = await invoke<VocabularyEntry>('vocabulary_get', { id })
      if (generation === detailGeneration) {
        selected.value = entry
        error.value = ''
      }
    } catch (e) {
      if (generation === detailGeneration) error.value = String(e)
    }
  }
  async function edit(entry: VocabularyEntry) {
    if (busy.value || !(await flushDraft())) return
    const existing = drafts.value.find((d) => d.entryId === entry.id)
    if (existing) {
      apply(existing)
      editorOpen.value = true
      return
    }
    const { language, languageLabel, kind, text, meaning, meaningLanguage, note } = entry
    apply({
      id: crypto.randomUUID(),
      entryId: entry.id,
      baseRevision: entry.revision,
      fields: { language, languageLabel, kind, text, meaning, meaningLanguage, note },
      occurrence: null,
      allowDuplicate: false,
      tagText: entry.tags.join(', '),
      requestId: crypto.randomUUID(),
    })
    editorOpen.value = true
  }
  async function compareLatest() {
    if (!editor.value?.entryId) return
    try {
      latestVersion.value = await invoke<VocabularyEntry>('vocabulary_get', {
        id: editor.value.entryId,
      })
    } catch (e) {
      error.value = String(e)
    }
  }
  async function acceptComparedDraft() {
    if (!editor.value || latestVersion.value?.id !== editor.value.entryId) return
    editor.value.baseRevision = latestVersion.value.revision
    editor.value.requestId = crypto.randomUUID()
    dirty = true
    latestVersion.value = null
    await save()
  }
  async function resume(draft: EditDraft) {
    if (busy.value || !(await flushDraft())) return
    apply(draft)
    editorOpen.value = true
  }
  async function closeEditor() {
    if (!busy.value && (await flushDraft())) editorOpen.value = false
  }
  async function discard() {
    if (!editor.value || busy.value) return
    clearTimeout(timer)
    if (saving) await saving
    try {
      await invoke('vocabulary_discard_draft', { id: editor.value.id })
      dirty = false
      apply(null)
      editorOpen.value = false
      await refreshDrafts()
      draftError.value = ''
    } catch (e) {
      error.value = String(e)
    }
  }
  async function perform<T>(action: () => Promise<T>): Promise<T | undefined> {
    if (busy.value) return
    busy.value = true
    error.value = ''
    const task = (async () => {
      try {
        return await action()
      } catch (e) {
        error.value = String(e)
        return undefined
      }
    })()
    operation = task
    try {
      return await task
    } finally {
      operation = undefined
      busy.value = false
    }
  }
  async function save() {
    return perform(async () => {
      if (!editor.value || !(await flushDraft())) return
      const draft = clone(editor.value)
      const result = await invoke<SaveResult>('vocabulary_save', {
        request: {
          requestId: draft.requestId,
          id: draft.entryId,
          expectedRevision: draft.baseRevision,
          fields: draft.fields,
          occurrence: draft.occurrence,
          draftId: draft.id,
          allowDuplicate: draft.allowDuplicate,
          tags: parseTags(draft.tagText, 20, 50),
        },
      })
      if (result.duplicate) {
        duplicate.value = result.entry
        notice.value = '这处原句已收藏。可以查看原收藏，或明确保存另一种意思。'
        return
      }
      selected.value = result.entry
      dirty = false
      apply(null)
      editorOpen.value = false
      notice.value = '词句已保存'
      await refreshDrafts()
      await refresh()
    })
  }
  async function attach(entry: VocabularyEntry) {
    return perform(async () => {
      if (!editor.value?.occurrence || !(await flushDraft())) return
      const keepDraft = Boolean(
        editor.value.fields.meaning.trim() ||
        editor.value.fields.note.trim() ||
        editor.value.tagText.trim(),
      )
      selected.value = await invoke<VocabularyEntry>('vocabulary_add_occurrence', {
        request: {
          requestId: mutationId([
            'occurrence',
            entry.id,
            entry.revision,
            editor.value.id,
            editor.value.occurrence,
          ]),
          id: entry.id,
          expectedRevision: entry.revision,
          source: clone(editor.value.occurrence),
          draftId: keepDraft ? null : editor.value.id,
        },
      })
      dirty = false
      apply(null)
      editorOpen.value = false
      notice.value = keepDraft
        ? '已补充原句，未提交的释义、注释和标签仍保留在草稿中'
        : '已补充原句，原有释义和注释保留'
      await refreshDrafts()
      await refresh()
    })
  }
  async function mutate(entry: VocabularyEntry, action: 'trash' | 'restore' | 'purge') {
    return perform(async () => {
      if (!(await flushDraft())) return
      await invoke(`vocabulary_${action}`, {
        request: {
          requestId: mutationId([action, entry.id, entry.revision]),
          id: entry.id,
          expectedRevision: entry.revision,
        },
      })
      if (action === 'purge') {
        if (selected.value?.id === entry.id) selected.value = null
        if (editor.value?.entryId === entry.id) {
          dirty = false
          apply(null)
          editorOpen.value = false
        }
      } else if (selected.value?.id === entry.id) await open(entry.id)
      notice.value =
        action === 'trash'
          ? '已移入回收站，可恢复'
          : action === 'restore'
            ? '已恢复词句'
            : '已彻底删除'
      await refreshDrafts()
      await refresh()
    })
  }
  function useAnswer(
    text: string,
    field: 'meaning' | 'note',
    target: { id: string; requestId: string },
  ) {
    if (
      !editor.value ||
      editor.value.id !== target.id ||
      editor.value.requestId !== target.requestId ||
      busy.value
    ) {
      error.value = '收藏草稿已有变化。请检查当前词句，再重新选择要写入的答案。'
      return
    }
    const fields: EntryFields = editor.value.fields
    fields[field] = field === 'note' && fields.note ? `${fields.note}\n${text}` : text
    editorOpen.value = true
    notice.value = '已填入草稿，请检查后保存'
  }
  return {
    initialized,
    tutorFocusRequest,
    wordFocusRequest,
    latestVersion,
    compareLatest,
    acceptComparedDraft,
    activeTab,
    loading,
    busy,
    error,
    draftError,
    notice,
    entries,
    total,
    activeCount,
    tags,
    languages,
    drafts,
    editor,
    selected,
    duplicate,
    editorOpen,
    query,
    initialize,
    refresh,
    flush,
    flushDraft,
    page,
    start,
    open,
    edit,
    resume,
    closeEditor,
    discard,
    save,
    attach,
    mutate,
    useAnswer,
  }
})
