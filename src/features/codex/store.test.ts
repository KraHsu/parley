import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { useCodexStore, type Conversation } from './store'
import { useBackendStore } from '../backends/store'
import type { TurnEvent } from '../backends/types'
import { useCodexConnectionsStore } from './connections'
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

describe('source navigation', () => {
  it('opens an exact saved message without relying on the history list and restores usage', async () => {
    const store = useCodexStore()
    await store.initializeWorkspace()
    const old = conversation('not-in-history', 'tutor')
    old.messages = [
      { id: 'source-message', role: 'assistant', text: 'original', usage: { total_tokens: 12 } },
    ]
    mock.invoke.mockImplementation(async (method: string, args) =>
      method === 'storage_read' ? old : defaultInvoke(method, args),
    )
    store.activeView = 'vocabulary'
    expect(await store.selectConversation(old.id, 'source-message')).toBe(true)
    expect(store.lanes.tutor.id).toBe(old.id)
    expect(store.mobilePane).toBe('tutor')
    expect(store.activeView).toBe('conversation')
    expect(store.sourceFocus).toMatchObject({
      conversationId: old.id,
      messageId: 'source-message',
      pane: 'tutor',
    })
    expect(store.lanes.tutor.messages[0]?.usage).toEqual({ total_tokens: 12 })
    expect(
      mock.invoke.mock.calls.some(
        ([method]) => method === 'codex_send' || method === 'backend_send',
      ),
    ).toBe(false)
  })
  it('keeps the word view and draft when the message is gone, without disabling model connections', async () => {
    const store = useCodexStore()
    await store.initializeWorkspace()
    store.activeView = 'vocabulary'
    store.lanes.main.draft = 'keep typing'
    expect(await store.selectConversation('older', 'missing')).toBe(false)
    expect(store.navigationError).toContain('原消息已不存在')
    expect(store.storageError).toBe('')
    expect(store.activeView).toBe('vocabulary')
    expect(store.lanes.main.id).toBe('main')
    expect(store.lanes.main.draft).toBe('keep typing')
    expect(store.sourceFocus).toBeNull()
    expect(store.navigating).toBe(false)
  })
  it('saves text entered during a slow read and excludes overlapping navigation and sends', async () => {
    const store = useCodexStore()
    await store.connect()
    const old = conversation('older', 'main')
    old.messages = [{ id: 'message', role: 'assistant', text: 'original' }]
    let release!: (value: Conversation) => void
    const pending = new Promise<Conversation>((resolve) => {
      release = resolve
    })
    const writes: string[] = []
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'storage_read') return pending
      if (method === 'storage_save')
        writes.push(args.drafts.find((d: { id: string }) => d.id === 'main')?.text ?? '')
      return defaultInvoke(method, args)
    })
    const navigating = store.selectConversation('older', 'message')
    await vi.waitFor(() =>
      expect(mock.invoke.mock.calls.some(([method]) => method === 'storage_read')).toBe(true),
    )
    store.lanes.main.draft = 'typed during read'
    expect(await store.selectConversation('another')).toBe(false)
    expect(await store.send('main', 'do not send')).toBe(false)
    release(old)
    expect(await navigating).toBe(true)
    expect(writes).toContain('typed during read')
    expect(store.lanes.main.id).toBe('older')
    expect(store.navigating).toBe(false)
  })
  it('serializes a slow new conversation with source navigation and preserves late drafts', async () => {
    const store = useCodexStore()
    await store.initializeWorkspace()
    let release!: (value: Conversation) => void
    const pending = new Promise<Conversation>((resolve) => {
      release = resolve
    })
    const writes: string[] = []
    mock.invoke.mockImplementation(async (method: string, args) => {
      if (method === 'storage_create') return pending
      if (method === 'storage_save')
        writes.push(args.drafts.find((d: { id: string }) => d.id === 'main')?.text ?? '')
      return defaultInvoke(method, args)
    })
    const reset = store.reset('main')
    await vi.waitFor(() =>
      expect(mock.invoke.mock.calls.some(([method]) => method === 'storage_create')).toBe(true),
    )
    store.lanes.main.draft = 'saved on previous conversation'
    expect(await store.selectConversation('source', 'message')).toBe(false)
    release(conversation('new', 'main'))
    await reset
    expect(writes).toContain('saved on previous conversation')
    expect(store.lanes.main.id).toBe('new')
    expect(store.navigating).toBe(false)
  })
  it('does not replace a generating lane and lets the other lane finish untouched', async () => {
    const store = useCodexStore()
    await store.initializeWorkspace()
    store.lanes.tutor.busy = true
    store.lanes.tutor.messages = [{ id: 'running', role: 'assistant', text: 'partial' }]
    expect(await store.selectConversation('tutor', 'running')).toBe(false)
    expect(store.navigationError).toContain('停止')
    expect(await store.selectConversation('main')).toBe(true)
    expect(store.lanes.tutor.busy).toBe(true)
    expect(store.lanes.tutor.messages[0]?.text).toBe('partial')
  })
})

describe('API conversation routing', () => {
  async function setup() {
    const store = useCodexStore()
    await store.connect()
    const backends = useBackendStore()
    backends.profiles = [
      {
        id: 'api',
        revision: 1,
        config: {
          name: 'Fixture API',
          kind: 'openai_responses',
          provider: 'openai',
          endpoint: 'https://api.example.invalid/v1',
          binaryPath: '',
          enabled: true,
        },
      },
    ]
    backends.state('api').credential = { configured: true, persistence: 'session' }
    store.lanes.main.backend = { profileId: 'api', profileRevision: 1, kind: 'openai_responses' }
    store.mainModel = 'manual-api-model'
    return store
  }
  it('isolates concurrent Codex and API replies and ignores stale projections', async () => {
    const store = await setup()
    await Promise.all([store.send('main', 'Hello API'), store.send('tutor', 'Explain hello')])
    const call = mock.invoke.mock.calls.find((c) => c[0] === 'backend_send')![1]
    const event: TurnEvent = {
      profileId: 'api',
      profileRevision: 1,
      conversationId: 'main',
      pane: 'main',
      turnId: 'turn-api',
      requestId: call.request.messageId,
      messageId: 'api-answer',
      sequence: 2,
      status: 'streaming',
      text: 'API answer',
      usage: { input_tokens: 10, output_tokens: 2 },
      error: null,
      notice: null,
    }
    call.events.onmessage(event)
    call.events.onmessage({ ...event, sequence: 1, text: 'stale' })
    call.events.onmessage({ ...event, sequence: 3, conversationId: 'other', text: 'wrong' })
    emit('item/agentMessage/delta', {
      pane: 'tutor',
      conversationId: 'tutor',
      itemId: 'codex-answer',
      delta: 'Codex answer',
    })
    expect(store.lanes.main.messages.at(-1)?.text).toBe('API answer')
    expect(store.lanes.tutor.messages.at(-1)?.text).toBe('Codex answer')
    call.events.onmessage({
      ...event,
      sequence: 3,
      status: 'complete',
      text: 'API final',
      usage: null,
    })
    call.events.onmessage({ ...event, sequence: 4, text: 'late delta' })
    expect(store.lanes.main.messages.at(-1)?.text).toBe('API final')
    expect(store.lanes.main.messages.at(-1)?.usage).toEqual({ input_tokens: 10, output_tokens: 2 })
    expect(store.lanes.main.busy).toBe(false)
    expect(store.lanes.tutor.busy).toBe(true)
  })
  it('stops the exact API request without stopping the Codex process', async () => {
    const store = await setup()
    await store.send('main', 'Hello API')
    const request = mock.invoke.mock.calls.find((c) => c[0] === 'backend_send')![1].request
    await store.stop('main')
    expect(mock.invoke).toHaveBeenCalledWith('backend_stop', {
      conversationId: 'main',
      requestId: request.messageId,
    })
    expect(mock.invoke.mock.calls.some((c) => c[0] === 'codex_stop')).toBe(false)
  })
})

describe('multiple Codex profiles', () => {
  async function setup() {
    const store = useCodexStore()
    await store.initializeWorkspace()
    const backends = useBackendStore()
    backends.profiles = ['codex-a', 'codex-b'].map((id) => ({
      id,
      revision: 2,
      config: {
        name: id,
        kind: 'codex',
        provider: 'openai',
        endpoint: '',
        binaryPath: `/bin/${id}`,
        enabled: true,
      },
    }))
    const connections = useCodexConnectionsStore()
    await store.connectCodexProfile('codex-a')
    await store.connectCodexProfile('codex-b')
    store.lanes.main.backend = { profileId: 'codex-a', profileRevision: 2, kind: 'codex' }
    store.lanes.tutor.backend = { profileId: 'codex-b', profileRevision: 2, kind: 'codex' }
    store.mainModel = 'main-model'
    store.tutorModel = 'gpt-5.6-luna'
    return { store, backends, connections }
  }
  it('routes simultaneous sends and overlapping item IDs by profile and disconnects only that profile', async () => {
    const { store, connections } = await setup()
    expect(await store.send('main', 'Hello')).toBe(true)
    expect(await store.send('tutor', 'Explain')).toBe(true)
    expect(mock.invoke).toHaveBeenCalledWith(
      'codex_send',
      expect.objectContaining({
        profileId: 'codex-a',
        request: expect.objectContaining({ pane: 'main' }),
      }),
    )
    expect(mock.invoke).toHaveBeenCalledWith(
      'codex_send',
      expect.objectContaining({
        profileId: 'codex-b',
        request: expect.objectContaining({ pane: 'tutor' }),
      }),
    )
    emit(
      'item/agentMessage/delta',
      { pane: 'main', conversationId: 'main', itemId: 'same', delta: 'Main answer' },
      0,
    )
    emit(
      'item/agentMessage/delta',
      { pane: 'tutor', conversationId: 'tutor', itemId: 'same', delta: 'Tutor answer' },
      1,
    )
    emit(
      'item/agentMessage/delta',
      { pane: 'tutor', conversationId: 'tutor', itemId: 'same', delta: 'Wrong profile' },
      0,
    )
    expect(store.lanes.main.messages.at(-1)?.text).toBe('Main answer')
    expect(store.lanes.tutor.messages.at(-1)?.text).toBe('Tutor answer')
    await connections.get('codex-a').disconnect()
    expect(mock.invoke).toHaveBeenCalledWith('codex_disconnect', { profileId: 'codex-a' })
    expect(store.lanes.main.busy).toBe(false)
    expect(store.lanes.main.messages.at(-1)?.status).toBe('interrupted')
    expect(store.lanes.tutor.busy).toBe(true)
    emit('item/completed', { pane: 'main', item: { id: 'same', text: 'Late answer' } }, 0)
    expect(store.lanes.main.messages.at(-1)?.text).toBe('Main answer')
    emit('item/agentMessage/delta', { pane: 'tutor', itemId: 'same', delta: ' continues' }, 1)
    expect(store.lanes.tutor.messages.at(-1)?.text).toBe('Tutor answer continues')
    await store.stop('tutor')
    expect(mock.invoke).toHaveBeenCalledWith('codex_stop', {
      pane: 'tutor',
      requestId: expect.any(String),
      profileId: 'codex-b',
    })
  })
  it('applies the common durable turn channel to both Codex panes and rejects post-final output', async () => {
    const { store } = await setup()
    await store.send('main', 'Hello')
    await store.send('tutor', 'Explain')
    const calls = mock.invoke.mock.calls.filter(([method]) => method === 'codex_send')
    for (const [index, pane] of (['main', 'tutor'] as const).entries()) {
      const args = calls[index]![1]
      const event: TurnEvent = {
        profileId: args.profileId,
        profileRevision: 2,
        conversationId: pane,
        pane,
        turnId: `local-turn-${pane}`,
        requestId: args.request.messageId,
        messageId: `local-answer-${pane}`,
        sequence: 1,
        status: 'streaming',
        text: 'partial',
        usage: null,
        error: null,
        notice: null,
      }
      args.events.onmessage(event)
      expect(store.lanes[pane].messages.at(-1)?.text).toBe('partial')
      store.lanes[pane].error = 'Temporary connection notice'
      args.events.onmessage({
        ...event,
        sequence: 2,
        status: 'complete',
        text: 'final',
        usage: { last: { inputTokens: 20, outputTokens: 3 } },
      })
      args.events.onmessage({ ...event, sequence: 3, text: 'late overwrite' })
      expect(store.lanes[pane].busy).toBe(false)
      expect(store.lanes[pane].messages.at(-1)?.text).toBe('final')
      expect(store.lanes[pane].error).toBe('')
      expect(store.lanes[pane].messages.at(-1)?.usage).toEqual({
        last: { inputTokens: 20, outputTokens: 3 },
      })
    }
    expect(store.lanes.main.messages.at(-1)?.id).not.toBe(store.lanes.tutor.messages.at(-1)?.id)
  })
  it('accepts an already saved final turn after the connection notice arrives first', async () => {
    const { store, connections } = await setup()
    await store.send('main', 'Hello')
    const args = mock.invoke.mock.calls.find(([method]) => method === 'codex_send')![1]
    const event: TurnEvent = {
      profileId: 'codex-a',
      profileRevision: 2,
      conversationId: 'main',
      pane: 'main',
      turnId: 'local-turn',
      requestId: args.request.messageId,
      messageId: 'local-answer',
      sequence: 1,
      status: 'streaming',
      text: 'partial',
      usage: null,
      error: null,
      notice: null,
    }
    args.events.onmessage(event)
    await connections.get('codex-a').disconnect()
    args.events.onmessage({
      ...event,
      sequence: 2,
      status: 'interrupted',
      text: 'final saved partial',
    })
    expect(store.lanes.main.messages.at(-1)?.text).toBe('final saved partial')
    expect(store.lanes.main.messages.at(-1)?.status).toBe('interrupted')
    args.events.onmessage({ ...event, sequence: 3, text: 'wrong late write' })
    expect(store.lanes.main.messages.at(-1)?.text).toBe('final saved partial')
  })
  it('ignores the wrong profile or revision and requires reconnect after editing settings', async () => {
    const { store, backends } = await setup()
    for (const envelope of [
      { profileId: 'codex-b', profileRevision: 2 },
      { profileId: 'codex-a', profileRevision: 1 },
    ]) {
      mock.callbacks[0]!.onmessage({
        ...envelope,
        method: 'item/agentMessage/delta',
        params: { pane: 'main', itemId: 'wrong', delta: 'Wrong' },
      })
    }
    expect(store.lanes.main.messages).toEqual([])
    backends.profiles[0]!.revision = 3
    expect(store.isReady('main')).toBe(false)
    expect(store.isReady('tutor')).toBe(true)
    expect(await store.send('main', 'Do not send')).toBe(false)
    expect(mock.invoke).not.toHaveBeenCalledWith('codex_send', expect.anything())
  })
  it('scopes account, model, limits and login operations to the selected configuration', async () => {
    const { connections } = await setup()
    for (const section of ['account', 'models', 'limits']) {
      expect(mock.invoke).toHaveBeenCalledWith('codex_status', { section, profileId: 'codex-b' })
    }
    mock.invoke.mockImplementation(async (method, args) => {
      if (method === 'codex_status') return { account: null, models: [], limits: null }
      if (method === 'codex_login')
        return { loginId: 'login-b', authUrl: 'https://auth.openai.com/authorize' }
      return defaultInvoke(method, args)
    })
    const b = connections.get('codex-b')
    await b.signIn()
    expect(mock.invoke).toHaveBeenCalledWith('codex_login', { profileId: 'codex-b' })
    expect(mock.invoke).toHaveBeenCalledWith('codex_open_login', {
      profileId: 'codex-b',
      url: 'https://auth.openai.com/authorize',
    })
    await b.cancelLogin()
    expect(mock.invoke).toHaveBeenCalledWith('codex_cancel_login', {
      profileId: 'codex-b',
      loginId: 'login-b',
    })
    expect(connections.get('codex-a').ready).toBe(true)
  })
  it('an old login cancellation cannot clear a newer login after reconnect', async () => {
    const { connections } = await setup()
    const connection = connections.get('codex-b')
    connection.login = { loginId: 'old-login', authUrl: 'https://auth.openai.com/authorize' }
    connection.loggingIn = true
    let finish!: () => void
    mock.invoke.mockImplementation((method, args) =>
      method === 'codex_cancel_login'
        ? new Promise<void>((resolve) => {
            finish = resolve
          })
        : Promise.resolve(defaultInvoke(method, args)),
    )
    const cancelled = connection.cancelLogin()
    await connection.disconnect()
    await connection.connect('/bin/codex-b', 2)
    connection.login = { loginId: 'new-login', authUrl: 'https://auth.openai.com/authorize' }
    connection.loggingIn = true
    finish()
    await cancelled
    expect(connection.login?.loginId).toBe('new-login')
    expect(connection.loggingIn).toBe(true)
  })
  it('cancel during initialization invalidates both its result and old channel before reconnecting', async () => {
    let finish!: (value: unknown) => void
    mock.invoke.mockImplementation((method, args) =>
      method === 'codex_connect'
        ? new Promise((resolve) => {
            finish = resolve
          })
        : Promise.resolve(defaultInvoke(method, args)),
    )
    const connection = useCodexConnectionsStore().get('codex-a')
    const pending = connection.connect('/bin/codex-a', 2)
    expect(connection.connecting).toBe(true)
    await connection.disconnect()
    finish({ profileRevision: 2 })
    await pending
    expect(connection.connected).toBe(false)
    expect(connection.connecting).toBe(false)
    expect(mock.invoke).not.toHaveBeenCalledWith('codex_status', expect.anything())
    mock.invoke.mockImplementation(async (method, args) => defaultInvoke(method, args))
    await connection.connect('/bin/codex-a', 2)
    emit('connection/closed', { message: 'Old connection exited' }, 0)
    expect(connection.ready).toBe(true)
    expect(connection.error).toBe('')
  })
})
