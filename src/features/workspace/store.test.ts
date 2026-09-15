import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useWorkspaceStore } from './store'
import { useChatStore } from '../chat/store'
import type { Conversation } from '../chat/types'
import type { Workspace } from './types'
const mock = vi.hoisted(() => ({ invoke: vi.fn() }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: mock.invoke, Channel: class {} }))
vi.mock('../../shared/desktop', () => ({ isDesktop: () => true }))
const conversation = (id: string, pane: 'main' | 'tutor'): Conversation => ({
  id,
  pane,
  title: id,
  model: 'saved-model',
  targetLanguage: 'fr',
  nativeLanguage: 'zh-CN',
  mode: 'conversation',
  draft: `draft-${id}`,
  threadId: null,
  signature: '',
  status: 'idle',
  updatedAt: 1,
  messages: [],
  backend: { profileId: 'saved-api', profileRevision: 1, kind: 'openai_responses' },
})
const saved = (): Workspace => ({
  preferences: {
    codexPath: '',
    nativeLanguage: 'zh-CN',
    targetLanguage: 'fr',
    mainModel: 'saved-model',
    tutorModel: 'saved-model',
    mainId: 'main',
    tutorId: 'tutor',
    tutorMode: 'explain',
    activeView: 'vocabulary',
    mobilePane: 'tutor',
  },
  main: conversation('main', 'main'),
  tutor: conversation('tutor', 'tutor'),
  history: [conversation('main', 'main'), conversation('tutor', 'tutor')],
  path: '/fixture/test.sqlite3',
})
beforeEach(() => {
  setActivePinia(createPinia())
  mock.invoke.mockReset().mockImplementation(async (method: string) => {
    if (method === 'storage_load') return saved()
    if (method === 'storage_list') return saved().history
    if (method === 'storage_save') return null
    throw new Error('Model service unavailable')
  })
})
describe('local workspace', () => {
  it('restores and saves learning preferences and drafts without creating any model store', async () => {
    const pinia = createPinia()
    setActivePinia(pinia)
    const workspace = useWorkspaceStore()
    await workspace.initialize()
    expect(workspace.ready).toBe(true)
    expect(workspace.activeView).toBe('vocabulary')
    expect(workspace.documents.main.draft).toBe('draft-main')
    expect(workspace.history).toHaveLength(2)
    workspace.documents.tutor.draft = 'Comment dit-on ceci ?'
    workspace.mobilePane = 'main'
    expect(await workspace.flush()).toBe(true)
    expect(mock.invoke).toHaveBeenLastCalledWith(
      'storage_save',
      expect.objectContaining({
        preferences: expect.objectContaining({ mobilePane: 'main' }),
        drafts: expect.arrayContaining([{ id: 'tutor', text: 'Comment dit-on ceci ?' }]),
      }),
    )
    expect(mock.invoke.mock.calls.every(([method]) => String(method).startsWith('storage_'))).toBe(
      true,
    )
    expect(pinia.state.value.chat).toBeUndefined()
    expect(pinia.state.value.backends).toBeUndefined()
    expect(pinia.state.value['codex-connection']).toBeUndefined()
  })
  it('initializes once for simultaneous consumers and never saves a failed database load', async () => {
    let reject!: (reason: Error) => void
    mock.invoke.mockImplementation(
      () =>
        new Promise((_resolve, fail) => {
          reject = fail
        }),
    )
    const workspace = useWorkspaceStore()
    const a = workspace.initialize(),
      b = workspace.initialize()
    expect(mock.invoke).toHaveBeenCalledTimes(1)
    reject(new Error('Database unavailable'))
    await Promise.all([a, b])
    expect(workspace.initialized).toBe(false)
    expect(await workspace.flush()).toBe(false)
    expect(workspace.storageError).toContain('Database unavailable')
    expect(mock.invoke).toHaveBeenCalledTimes(1)
    mock.invoke.mockImplementation(async (method) => (method === 'storage_load' ? saved() : null))
    await workspace.retryStorage()
    expect(workspace.ready).toBe(true)
    expect(workspace.documents.main.draft).toBe('draft-main')
  })
  it('attaches chat to already loaded documents without losing edits when model setup fails', async () => {
    const workspace = useWorkspaceStore()
    await workspace.initialize()
    workspace.documents.main.draft = 'Une modification locale'
    const chat = useChatStore()
    await chat.initializeWorkspace()
    expect(mock.invoke.mock.calls.filter(([method]) => method === 'storage_load')).toHaveLength(1)
    expect(chat.lanes.main.draft).toBe('Une modification locale')
    expect(workspace.ready).toBe(true)
    expect(chat.isReady('main')).toBe(false)
    chat.lanes.main.draft = 'Une autre modification'
    expect(workspace.documents.main.draft).toBe('Une autre modification')
    chat.lanes.main.vocabularyTarget = { id: 'old-card', requestId: 'old-question' }
    const oldRequest = chat.lanes.main.request
    const restored = conversation('restored', 'main')
    restored.status = 'interrupted'
    restored.messages = [
      { id: 'answer', role: 'assistant', text: 'Saved partial answer', status: 'interrupted' },
    ]
    workspace.adopt('main', restored, true)
    expect(chat.lanes.main.request).toBeGreaterThan(oldRequest)
    expect(chat.lanes.main.vocabularyTarget).toBeUndefined()
    expect(chat.lanes.main.error).toContain('上次回复已中断')
    expect(chat.lanes.main.messages).toEqual(restored.messages)
    expect(await workspace.flush()).toBe(true)
  })
  it('ignores a stale history failure after a newer read has succeeded', async () => {
    const workspace = useWorkspaceStore()
    await workspace.initialize()
    let fail!: (error: Error) => void
    mock.invoke.mockImplementationOnce(
      () =>
        new Promise((_resolve, reject) => {
          fail = reject
        }),
    )
    const old = workspace.refreshHistory()
    mock.invoke.mockResolvedValueOnce([conversation('latest', 'main')])
    await workspace.refreshHistory()
    fail(new Error('Old read failed'))
    await old
    expect(workspace.history[0]?.id).toBe('latest')
    expect(workspace.storageError).toBe('')
    expect(workspace.ready).toBe(true)
  })
})
