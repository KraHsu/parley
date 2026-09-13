import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useCodexStore, type Conversation } from './store'
import { useSettingsStore } from '../settings/store'
const mock = vi.hoisted(() => ({
  invoke: vi.fn(),
  desktop: true,
  callbacks: [] as { onmessage: (event: unknown) => void }[],
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mock.invoke,
  Channel: class {
    onmessage = (_event: unknown) => {}
    constructor() {
      mock.callbacks.push(this)
    }
  },
}))
vi.mock('../../shared/desktop', () => ({ isDesktop: () => mock.desktop }))
const status = {
  account: { type: 'chatgpt', email: null, planType: 'plus' },
  models: [
    { id: 'default', model: 'main-model', displayName: 'Main', isDefault: true },
    { id: 'luna', model: 'gpt-5.6-luna', displayName: 'Luna', isDefault: false },
  ],
  limits: null,
}
const conversation = (id: string, pane: 'main' | 'tutor'): Conversation => ({
  id,
  pane,
  title: '新的对话',
  model: '',
  targetLanguage: 'en',
  nativeLanguage: 'zh-CN',
  mode: pane === 'main' ? 'conversation' : 'express',
  draft: '',
  threadId: null,
  signature: '',
  status: 'idle',
  updatedAt: 1,
  messages: [],
})
const workspace = () => ({
  preferences: {
    nativeLanguage: 'zh-CN',
    targetLanguage: 'en',
    mainModel: '',
    tutorModel: '',
    mainId: 'main',
    tutorId: 'tutor',
    tutorMode: 'express',
    activeView: 'conversation',
    mobilePane: 'main',
  },
  main: conversation('main', 'main'),
  tutor: conversation('tutor', 'tutor'),
  history: [conversation('main', 'main'), conversation('tutor', 'tutor')],
  path: '/test/parley.sqlite3',
})
function defaultInvoke(method: string, args?: { id?: string; pane?: 'main' | 'tutor' }) {
  if (method === 'codex_status') return status
  if (method === 'storage_load') return workspace()
  if (method === 'storage_create') return conversation(args!.id!, args!.pane!)
  if (method === 'storage_list') return workspace().history
  if (method === 'storage_read')
    return conversation(args!.id!, args!.id === 'tutor' ? 'tutor' : 'main')
  return null
}
beforeEach(() => {
  setActivePinia(createPinia())
  mock.callbacks.length = 0
  mock.desktop = true
  mock.invoke
    .mockReset()
    .mockImplementation(async (method: string, args) => defaultInvoke(method, args))
})
const emit = (method: string, params: unknown, index = 0) =>
  mock.callbacks[index]!.onmessage({ method, params })
describe('Codex workspace', () => {
  it('uses the account catalog and keeps streamed replies in their own panel', async () => {
    const store = useCodexStore()
    await store.connect()
    expect(store.ready).toBe(true)
    expect(store.tutorModel).toBe('gpt-5.6-luna')
    await Promise.all([store.send('main', 'Hello'), store.send('tutor', 'Explain hello')])
    emit('item/agentMessage/delta', { pane: 'main', itemId: 'a', delta: 'Hi' })
    emit('item/agentMessage/delta', { pane: 'tutor', itemId: 'b', delta: '你' })
    emit('item/completed', { pane: 'tutor', item: { id: 'b', text: '你好' } })
    emit('turn/completed', { pane: 'main', turn: { status: 'completed' } })
    expect(store.lanes.main.messages.at(-1)?.text).toBe('Hi')
    expect(store.lanes.tutor.messages.at(-1)?.text).toBe('你好')
    expect(store.lanes.tutor.messages).toHaveLength(2)
    expect(store.lanes.main.busy).toBe(false)
    expect(store.lanes.tutor.busy).toBe(true)
  })
  it('ignores events from an old connection and releases busy panels on process exit', async () => {
    const store = useCodexStore()
    await store.connect()
    await store.send('main', 'Hello')
    emit('connection/closed', { message: 'process exited' })
    expect(store.ready).toBe(false)
    expect(store.lanes.main.busy).toBe(false)
    await store.connect()
    emit('item/agentMessage/delta', { pane: 'main', itemId: 'stale', delta: 'stale' }, 0)
    expect(store.lanes.main.messages.some((m) => m.text === 'stale')).toBe(false)
    expect(store.ready).toBe(true)
  })
  it('does not re-enable a process that died while connect was resolving', async () => {
    const store = useCodexStore()
    mock.invoke.mockImplementation(async (method: string) => {
      if (method === 'codex_connect') emit('connection/closed', { message: 'died' })
      return defaultInvoke(method)
    })
    await store.connect()
    expect(store.ready).toBe(false)
    expect(store.connecting).toBe(false)
  })
  it('preserves the submitted text with an error and allows retry after a rejected send', async () => {
    const store = useCodexStore()
    await store.connect()
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'codex_send') throw new Error('quota exceeded')
      return defaultInvoke(method, args)
    })
    await store.send('main', 'My message')
    expect(store.lanes.main.messages[0]?.text).toBe('My message')
    expect(store.lanes.main.error).toContain('quota exceeded')
    expect(store.lanes.main.busy).toBe(false)
  })
  it('does not restore a pending login when completion precedes the RPC response', async () => {
    const store = useCodexStore()
    await store.connect()
    mock.invoke.mockImplementation(async (method: string) => {
      if (method === 'codex_login') {
        emit('account/login/completed', { success: true })
        return { loginId: 'finished', authUrl: 'https://auth.openai.com/authorize' }
      }
      return defaultInvoke(method)
    })
    await store.signIn()
    expect(store.login).toBeNull()
    expect(store.loggingIn).toBe(false)
    expect(mock.invoke).not.toHaveBeenCalledWith('codex_open_login', expect.anything())
  })
  it('does not invoke desktop commands from browser preview', async () => {
    mock.desktop = false
    const store = useCodexStore()
    await store.connect()
    expect(mock.invoke).not.toHaveBeenCalled()
    expect(store.error).toContain('desktop:dev')
  })
  it('hydrates saved preferences, drafts, and interrupted messages without sending anything', async () => {
    const saved = workspace()
    saved.preferences.targetLanguage = 'ja'
    saved.preferences.mainModel = 'old-model'
    saved.main.draft = '日本語の下書き'
    saved.main.status = 'interrupted'
    saved.main.messages = [
      { id: 'partial', role: 'assistant', text: '途中', status: 'interrupted' },
    ]
    mock.invoke.mockImplementation(async (method: string, args) =>
      method === 'storage_load' ? saved : defaultInvoke(method, args),
    )
    const store = useCodexStore()
    await store.initializeWorkspace()
    expect(useSettingsStore().targetLanguage).toBe('ja')
    expect(store.mainModel).toBe('old-model')
    expect(store.lanes.main.draft).toBe('日本語の下書き')
    expect(store.lanes.main.messages[0]?.text).toBe('途中')
    expect(store.lanes.main.busy).toBe(false)
    expect(mock.invoke.mock.calls.some((call) => call[0] === 'codex_send')).toBe(false)
  })
  it('serializes draft writes and flushes the newest text before returning', async () => {
    const store = useCodexStore()
    await store.initializeWorkspace()
    let release!: () => void
    const blocked = new Promise<void>((resolve) => {
      release = resolve
    })
    const writes: string[] = []
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'storage_save') {
        writes.push(args.drafts[0].text)
        if (writes.length === 1) await blocked
      }
      return defaultInvoke(method, args)
    })
    store.lanes.main.draft = 'first'
    store.lanes.main.draft = 'latest'
    const flushing = store.flush()
    release()
    expect(await flushing).toBe(true)
    expect(writes).toEqual(['first', 'latest'])
  })
  it('keeps unsaved drafts and blocks sending after a storage failure until retry succeeds', async () => {
    const store = useCodexStore()
    await store.connect()
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'storage_save') throw new Error('disk full')
      return defaultInvoke(method, args)
    })
    store.lanes.main.draft = 'Keep this'
    expect(await store.flush()).toBe(false)
    expect(store.storageError).toContain('disk full')
    expect(store.lanes.main.draft).toBe('Keep this')
    expect(await store.send('main', 'Keep this')).toBe(false)
    mock.invoke.mockImplementation(async (method: string, args) => defaultInvoke(method, args))
    await store.retryStorage()
    expect(store.storageError).toBe('')
    expect(store.ready).toBe(true)
  })
  it('does not overwrite the database with defaults when initial loading fails', async () => {
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'storage_load') throw new Error('invalid database')
      return defaultInvoke(method, args)
    })
    const store = useCodexStore()
    await store.initializeWorkspace()
    expect(store.initialized).toBe(false)
    expect(store.storageError).toContain('invalid database')
    expect(mock.invoke.mock.calls.some((call) => call[0] === 'storage_save')).toBe(false)
  })
  it('switches history only after the old draft is saved and restores its messages', async () => {
    const store = useCodexStore()
    await store.initializeWorkspace()
    const older = conversation('older', 'main')
    older.draft = 'old draft'
    older.messages = [
      { id: 'old-message', role: 'user', text: 'prior conversation', status: 'complete' },
    ]
    store.history.push(older)
    let savedOldDraft = false
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (
        method === 'storage_save' &&
        args.drafts.some(
          (d: { id: string; text: string }) => d.id === 'main' && d.text === 'new draft',
        )
      )
        savedOldDraft = true
      if (method === 'storage_read') {
        expect(savedOldDraft).toBe(true)
        return older
      }
      return defaultInvoke(method, args)
    })
    store.lanes.main.draft = 'new draft'
    await store.selectConversation('older')
    expect(store.lanes.main.id).toBe('older')
    expect(store.lanes.main.draft).toBe('old draft')
    expect(store.lanes.main.messages[0]?.text).toBe('prior conversation')
  })
})
