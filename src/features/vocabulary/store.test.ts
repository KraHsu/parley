import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { useVocabularyStore } from './store'
import type { EditDraft, EntryPage, VocabularyEntry } from './types'
const ipc = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: ipc.invoke }))
vi.mock('../../shared/desktop', () => ({ isDesktop: () => true }))
const empty: EntryPage = { entries: [], total: 0, languages: [], tags: [], activeCount: 0 }
function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}
function entry(id = 'entry'): VocabularyEntry {
  return {
    id,
    language: 'en',
    languageLabel: 'English',
    kind: 'word',
    text: 'hello',
    meaning: '你好',
    meaningLanguage: 'zh-CN',
    note: '',
    revision: 1,
    createdAt: 1,
    updatedAt: 1,
    deletedAt: null,
    occurrences: [],
    tags: [],
    cards: [],
  }
}
beforeEach(() => {
  setActivePinia(createPinia())
  ipc.invoke.mockReset()
  ipc.invoke.mockImplementation(async (command: string) =>
    command === 'vocabulary_load_drafts' ? [] : command === 'vocabulary_list' ? empty : undefined,
  )
})
describe('vocabulary workspace', () => {
  it('serializes changed drafts and flush waits for the newest version', async () => {
    const store = useVocabularyStore()
    await store.initialize()
    await store.start()
    const first = deferred<void>()
    const writes: EditDraft[] = []
    ipc.invoke.mockImplementation(async (command: string, args: { draft: EditDraft }) => {
      if (command === 'vocabulary_save_draft') {
        writes.push(structuredClone(args.draft))
        if (writes.length === 1) await first.promise
      }
    })
    store.editor!.fields.text = 'first'
    const flush = store.flush()
    store.editor!.fields.text = 'second'
    first.resolve()
    expect(await flush).toBe(true)
    expect(writes.map((d) => d.fields.text)).toEqual(['first', 'second'])
    expect(writes[0]!.requestId).not.toBe(writes[1]!.requestId)
  })
  it('failed writes keep the form and retry with the same request ID', async () => {
    const store = useVocabularyStore()
    await store.initialize()
    await store.start()
    store.editor!.fields.text = 'hello'
    const requests: unknown[] = []
    let attempt = 0
    ipc.invoke.mockImplementation(async (command: string, args: { request: unknown }) => {
      if (command === 'vocabulary_save') {
        requests.push(args.request)
        if (++attempt === 1) throw Error('disk full')
        return { entry: entry(), duplicate: false }
      }
      if (command === 'vocabulary_list') return empty
      if (command === 'vocabulary_load_drafts') return []
    })
    await store.save()
    expect(store.error).toContain('disk full')
    expect(store.editor!.fields.text).toBe('hello')
    await store.save()
    expect(requests[0]).toEqual(requests[1])
    expect(store.editor).toBeNull()
    expect(store.selected?.text).toBe('hello')
  })
  it('preserves a second-meaning draft when the source was already collected', async () => {
    const store = useVocabularyStore()
    await store.initialize()
    await store.start()
    store.editor!.fields.text = 'hello'
    store.editor!.fields.meaning = 'different sense'
    ipc.invoke.mockImplementation(async (command: string) =>
      command === 'vocabulary_save' ? { entry: entry(), duplicate: true } : undefined,
    )
    await store.save()
    expect(store.editor!.fields.meaning).toBe('different sense')
    expect(store.editorOpen).toBe(true)
    expect(store.duplicate?.id).toBe('entry')
  })
  it('keeps entered meaning, notes and tags as a draft when only attaching a source', async () => {
    const store = useVocabularyStore()
    await store.initialize()
    await store.start(
      {
        sourceKind: 'terminal',
        conversationId: null,
        messageId: null,
        threadId: 'thread',
        turnId: 'turn',
        itemId: 'item',
        role: 'assistant',
        selectedText: 'hello',
        snapshot: 'hello',
        start: 0,
        end: 5,
        truncated: false,
        locatorVersion: 1,
      },
      'en',
    )
    store.editor!.fields.meaning = 'another meaning'
    store.editor!.tagText = 'daily'
    ipc.invoke.mockImplementation(async (command: string, args) => {
      if (command === 'vocabulary_add_occurrence') {
        expect(args.request.draftId).toBeNull()
        return entry()
      }
      if (command === 'vocabulary_list') return empty
      if (command === 'vocabulary_load_drafts') return []
    })
    await store.attach(entry())
    expect(store.notice).toContain('仍保留在草稿')
    expect(ipc.invoke.mock.calls.some(([command]) => command === 'vocabulary_discard_draft')).toBe(
      false,
    )
  })
  it('ignores search results invalidated while debounce is pending', async () => {
    const store = useVocabularyStore()
    await store.initialize()
    const old = deferred<EntryPage>()
    ipc.invoke.mockReturnValueOnce(old.promise)
    const read = store.refresh()
    store.query.search = 'new'
    await nextTick()
    old.resolve({
      entries: [entry('stale')],
      total: 1,
      languages: ['en'],
      tags: [],
      activeCount: 1,
    })
    await read
    expect(store.entries).toEqual([])
    // Dispose the scheduled search through fake time-free completion.
    await new Promise((r) => setTimeout(r, 200))
  })
  it('does not apply a late answer to a different or edited draft', async () => {
    const store = useVocabularyStore()
    await store.initialize()
    await store.start()
    store.editor!.fields.text = 'hello'
    const target = { id: store.editor!.id, requestId: store.editor!.requestId }
    store.editor!.fields.meaning = 'my own answer'
    store.useAnswer('late model answer', 'meaning', target)
    expect(store.editor!.fields.meaning).toBe('my own answer')
    expect(store.error).toContain('已有变化')
    const current = { id: store.editor!.id, requestId: store.editor!.requestId }
    store.useAnswer('an example', 'note', current)
    expect(store.editor!.fields.note).toBe('an example')
    await store.flush()
  })
  it('close flush waits for a pending confirmed save', async () => {
    const store = useVocabularyStore()
    await store.initialize()
    await store.start()
    store.editor!.fields.text = 'hello'
    const pending = deferred<{ entry: VocabularyEntry; duplicate: boolean }>()
    ipc.invoke.mockImplementation(async (command: string) =>
      command === 'vocabulary_save'
        ? pending.promise
        : command === 'vocabulary_list'
          ? empty
          : command === 'vocabulary_load_drafts'
            ? []
            : undefined,
    )
    const save = store.save()
    await nextTick()
    let finished = false
    const flush = store.flush().then((ok) => {
      finished = ok
    })
    await nextTick()
    expect(finished).toBe(false)
    pending.resolve({ entry: entry(), duplicate: false })
    await save
    await flush
    expect(finished).toBe(true)
  })
})
