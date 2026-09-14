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
    codexPath: '/user/bin/codex',
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
function defaultInvoke(
  method: string,
  args?: { id?: string; pane?: 'main' | 'tutor'; section?: string },
) {
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
  it('ignores an old conversation event after switching the pane', async () => {
    const store = useCodexStore()
    await store.connect()
    emit('item/agentMessage/delta', {
      pane: 'main',
      conversationId: 'different-conversation',
      itemId: 'late',
      delta: 'wrong answer',
    })
    expect(store.lanes.main.messages).toEqual([])
    emit('item/completed', {
      pane: 'main',
      conversationId: 'main',
      item: { id: 'final', text: 'saved answer' },
    })
    emit('item/agentMessage/delta', {
      pane: 'main',
      conversationId: 'main',
      itemId: 'final',
      delta: 'duplicate tail',
    })
    expect(store.lanes.main.messages[0]?.text).toBe('saved answer')
  })
  it('Codex disconnect leaves a different backend lane untouched', async () => {
    const store = useCodexStore()
    await store.connect()
    store.lanes.tutor.backend = {
      profileId: 'independent-api',
      profileRevision: 1,
      kind: 'openai_responses',
    }
    store.lanes.tutor.busy = true
    store.lanes.tutor.messages.push({
      id: 'api-answer',
      role: 'assistant',
      text: 'partial',
      status: 'streaming',
    })
    emit('connection/closed', { message: 'Codex exited' })
    expect(store.lanes.tutor.busy).toBe(true)
    expect(store.lanes.tutor.messages[0]?.status).toBe('streaming')
    expect(store.lanes.tutor.error).toBe('')
  })
  it('binds late vocabulary answers to the draft version captured when sending', async () => {
    const store = useCodexStore()
    await store.connect()
    const target = { id: 'draft-a', requestId: 'version-a' }
    const sent = store.send('tutor', 'Explain this expression', 'explain', target)
    target.id = 'draft-b'
    target.requestId = 'version-b'
    expect(await sent).toBe(true)
    emit('item/agentMessage/delta', { pane: 'tutor', itemId: 'answer-a', delta: 'An answer' })
    expect(store.lanes.tutor.messages.at(-1)?.vocabularyTarget).toEqual({
      id: 'draft-a',
      requestId: 'version-a',
    })
    emit('turn/completed', { pane: 'tutor', turn: { status: 'completed' } })
    await store.send('tutor', 'Another question', 'explain')
    emit('item/agentMessage/delta', { pane: 'tutor', itemId: 'answer-b', delta: 'Next answer' })
    expect(store.lanes.tutor.messages.at(-1)?.vocabularyTarget).toBeUndefined()
  })
  it('saves the chosen executable and sends that exact path when connecting', async () => {
    const store = useCodexStore()
    await store.initializeWorkspace()
    expect(store.codexPath).toBe('/user/bin/codex')
    store.codexPath = '/user/My Tools/codex'
    await store.connect()
    expect(mock.invoke).toHaveBeenCalledWith(
      'storage_save',
      expect.objectContaining({
        preferences: expect.objectContaining({ codexPath: '/user/My Tools/codex' }),
      }),
    )
    expect(mock.invoke).toHaveBeenCalledWith(
      'codex_connect',
      expect.objectContaining({
        codexPath: '/user/My Tools/codex',
      }),
    )
  })
  it('requires an explicit executable for workspaces saved before the path setting existed', async () => {
    const saved = workspace()
    Reflect.deleteProperty(saved.preferences, 'codexPath')
    mock.invoke.mockImplementation(async (method: string, args) =>
      method === 'storage_load' ? saved : defaultInvoke(method, args),
    )
    const store = useCodexStore()
    await store.connect()
    expect(store.codexPath).toBe('')
    expect(store.error).toContain('完整路径')
    expect(mock.invoke.mock.calls.some((call) => call[0] === 'codex_connect')).toBe(false)
  })
  it('shows the local account before models load and does not wait for quota to enable chat', async () => {
    let resolveModels!: (value: unknown) => void
    const slowModels = new Promise((resolve) => {
      resolveModels = resolve
    })
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'codex_status' && args.section === 'models') return slowModels
      if (method === 'codex_status' && args.section === 'limits') return new Promise(() => {})
      return defaultInvoke(method, args)
    })
    const store = useCodexStore()
    const connection = store.connect()
    await vi.waitFor(() => expect(store.account?.type).toBe('chatgpt'))
    expect(store.needsLogin).toBe(false)
    expect(store.loadingModels).toBe(true)
    expect(store.ready).toBe(false)
    resolveModels({ models: status.models })
    await connection
    expect(store.ready).toBe(true)
    expect(store.connecting).toBe(false)
    expect(mock.invoke.mock.calls.some((call) => call[0] === 'codex_login')).toBe(false)
  })
  it('preserves local login when model and quota reads fail and does not start another login', async () => {
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'codex_status' && args.section !== 'account')
        throw new Error('network timeout')
      return defaultInvoke(method, args)
    })
    const store = useCodexStore()
    await store.connect()
    expect(store.connected).toBe(true)
    expect(store.needsLogin).toBe(false)
    expect(store.error).toContain('模型列表读取失败')
    expect(store.limitsError).toContain('额度暂时无法读取')
    await store.signIn()
    expect(store.loggingIn).toBe(false)
    expect(mock.invoke.mock.calls.some((call) => call[0] === 'codex_login')).toBe(false)
  })
  it('re-reads a completed local login even if the browser completion event was missed', async () => {
    let signedIn = false
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'codex_status' && args.section === 'account')
        return { account: signedIn ? status.account : null }
      if (method === 'codex_login')
        return { loginId: 'login', authUrl: 'https://auth.openai.com/test' }
      return defaultInvoke(method, args)
    })
    const store = useCodexStore()
    await store.connect()
    expect(store.needsLogin).toBe(true)
    expect(
      mock.invoke.mock.calls.some(
        (call) => call[0] === 'codex_status' && call[1].section === 'models',
      ),
    ).toBe(false)
    await store.signIn()
    expect(store.loggingIn).toBe(true)
    signedIn = true
    await store.refresh()
    expect(store.ready).toBe(true)
    expect(store.login).toBeNull()
    expect(store.loggingIn).toBe(false)
  })
  it('automatically includes terminal context only with a tutor question', async () => {
    const store = useCodexStore()
    await store.connect()
    store.terminalContext = 'assistant: How have you been?'
    await store.send('tutor', '解释当前回复', 'explain')
    expect(mock.invoke).toHaveBeenCalledWith(
      'codex_send',
      expect.objectContaining({
        request: expect.objectContaining({
          text: '解释当前回复',
          terminalContext: store.terminalContext,
        }),
      }),
    )
    await store.send('main', 'Hello')
    expect(mock.invoke).toHaveBeenCalledWith(
      'codex_send',
      expect.objectContaining({
        request: expect.objectContaining({ pane: 'main', terminalContext: null }),
      }),
    )
    expect(store.lanes.tutor.messages[0]?.text).toBe('解释当前回复')
  })
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
