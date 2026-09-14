import { computed, reactive, ref, watch, onScopeDispose } from 'vue'
import { defineStore, storeToRefs } from 'pinia'
import { Channel, invoke } from '@tauri-apps/api/core'
import { isDesktop } from '../../shared/desktop'
import { useSettingsStore } from '../settings/store'
import { useBackendStore } from '../backends/store'
import type { TurnEvent } from '../backends/types'

import { useCodexConnectionsStore } from '../codex/connections'
import { useCodexConnectionStore, type ServerEvent } from '../codex/connection'
import type { Pane, VocabularyAnswerTarget, Lane, Conversation, Workspace } from './types'
export type { Pane, Message, Conversation, VocabularyAnswerTarget } from './types'

const emptyLane = (): Lane => ({
  id: '',
  draft: '',
  title: '新的对话',
  messages: [],
  busy: false,
  error: '',
  signature: '',
  request: 0,
  backend: { profileId: 'codex-default', profileRevision: 1, kind: 'codex' },
})
const describe = (error: unknown) => (error instanceof Error ? error.message : String(error))

export const useChatStore = defineStore('chat', () => {
  const settings = useSettingsStore()
  const backends = useBackendStore()
  const connection = useCodexConnectionStore()
  const codexConnections = useCodexConnectionsStore()
  const {
    connected,
    connecting,
    error,
    notice,
    account,
    accountChecked,
    checkingAccount,
    loadingModels,
    limitsError,
    needsLogin,
    models,
    limits,
    login,
    loggingIn,
    label,
  } = storeToRefs(connection)
  const { disconnect, signIn, openLogin, cancelLogin } = connection
  const codexPath = ref('')
  const terminalContext = ref('')
  const mainModel = ref('')
  const tutorModel = ref('')
  const lanes = reactive<Record<Pane, Lane>>({ main: emptyLane(), tutor: emptyLane() })
  const initialized = ref(false)
  const loading = ref(false)
  const storageError = ref('')
  const dbPath = ref('')
  const history = ref<Conversation[]>([])
  const tutorMode = ref('express')
  const activeView = ref<'conversation' | 'vocabulary'>('conversation')
  const mobilePane = ref<Pane>('main')
  const navigating = ref(false)
  const navigationError = ref('')
  const sourceFocus = ref<{
    pane: Pane
    conversationId: string
    messageId: string
    request: number
  } | null>(null)
  let sourceFocusRequest = 0
  const saving = ref(false)
  const closing = ref(false)
  const workspaceReady = computed(() => initialized.value && !storageError.value && !closing.value)
  function isReady(pane: Pane) {
    if (!workspaceReady.value || navigating.value) return false
    const binding = lanes[pane].backend
    if (binding.kind !== 'codex')
      return backends.isReady(binding.profileId, binding.profileRevision)
    const runtime = codexConnections.get(binding.profileId)
    const profile = backends.profiles.find((p) => p.id === binding.profileId)
    return (
      runtime.ready &&
      (runtime.profileRevision === null || runtime.profileRevision === binding.profileRevision) &&
      (profile
        ? profile.config.enabled && profile.revision === binding.profileRevision
        : binding.profileId === 'codex-default')
    )
  }
  function paneLabel(pane: Pane) {
    const binding = lanes[pane].backend
    const profile = backends.profiles.find((p) => p.id === binding.profileId)
    const name = profile?.config.name ?? (binding.kind === 'codex' ? 'Codex' : '模型服务')
    if (profile && (!profile.config.enabled || profile.revision !== binding.profileRevision))
      return `${name} · 配置已变更`
    const status =
      binding.kind === 'codex'
        ? codexConnections.get(binding.profileId).label
        : isReady(pane)
          ? '可用'
          : '待配置'
    return `${name} · ${status}`
  }
  const ready = computed(() => isReady('main') || isReady('tutor'))
  let applying = false
  let dirty = false
  let savingTask: Promise<boolean> | null = null
  let loadingTask: Promise<void> | null = null
  let historyRequest = 0
  function snapshot() {
    return {
      preferences: {
        codexPath: codexPath.value,
        nativeLanguage: settings.nativeLanguage,
        targetLanguage: settings.targetLanguage,
        mainModel: mainModel.value,
        tutorModel: tutorModel.value,
        mainId: lanes.main.id || null,
        tutorId: lanes.tutor.id || null,
        tutorMode: tutorMode.value,
        activeView: activeView.value,
        mobilePane: mobilePane.value,
      },
      drafts: Object.values(lanes)
        .filter((lane) => lane.id)
        .map((lane) => ({ id: lane.id, text: lane.draft })),
    }
  }
  function scheduleSave() {
    if (!initialized.value || applying || !isDesktop()) return
    dirty = true
    if (!storageError.value) void flush()
  }
  async function flush(): Promise<boolean> {
    if (!isDesktop()) return true
    if (!initialized.value) return false
    if (savingTask) {
      const result = await savingTask
      return dirty && result ? flush() : result
    }
    savingTask = (async () => {
      saving.value = true
      try {
        while (dirty) {
          dirty = false
          try {
            await invoke('storage_save', snapshot())
            storageError.value = ''
          } catch (e) {
            dirty = true
            storageError.value = describe(e)
            return false
          }
        }
        return true
      } finally {
        saving.value = false
      }
    })()
    const result = await savingTask
    savingTask = null
    return dirty && result ? flush() : result
  }
  watch(
    () => [
      codexPath.value,
      settings.nativeLanguage,
      settings.targetLanguage,
      mainModel.value,
      tutorModel.value,
      tutorMode.value,
      activeView.value,
      mobilePane.value,
      lanes.main.id,
      lanes.main.draft,
      lanes.tutor.id,
      lanes.tutor.draft,
    ],
    scheduleSave,
    { flush: 'sync' },
  )
  function adopt(pane: Pane, conversation: Conversation, restoreSettings = false) {
    applying = true
    if (sourceFocus.value?.pane === pane) sourceFocus.value = null
    Object.assign(lanes[pane], emptyLane(), {
      id: conversation.id,
      title: conversation.title,
      draft: conversation.draft,
      messages: conversation.messages,
      signature: conversation.signature,
      backend: conversation.backend ?? emptyLane().backend,
      request: lanes[pane].request + 1,
      error:
        conversation.status === 'interrupted'
          ? '上次回复已中断，已保存的内容可继续查看。'
          : conversation.status === 'failed'
            ? '上次请求未完成，可查看记录或重新提问。'
            : '',
    })
    if (restoreSettings && conversation.model) {
      settings.nativeLanguage = conversation.nativeLanguage
      settings.targetLanguage = conversation.targetLanguage
      if (pane === 'main') mainModel.value = conversation.model
      else {
        tutorModel.value = conversation.model
        tutorMode.value = conversation.mode
      }
    }
    applying = false
    scheduleSave()
  }
  async function create(pane: Pane, profileId = lanes[pane].backend.profileId) {
    const id = crypto.randomUUID()
    if (!isDesktop())
      return {
        id,
        pane,
        title: '新的对话',
        model: '',
        targetLanguage: settings.targetLanguage,
        nativeLanguage: settings.nativeLanguage,
        mode: pane === 'main' ? 'conversation' : tutorMode.value,
        draft: '',
        threadId: null,
        signature: '',
        status: 'idle',
        updatedAt: Date.now(),
        messages: [],
      } satisfies Conversation
    return invoke<Conversation>('storage_create', { id, pane, profileId })
  }
  async function initializeWorkspace() {
    if (initialized.value) return
    if (loadingTask) return loadingTask
    loadingTask = (async () => {
      loading.value = true
      try {
        if (isDesktop()) {
          const workspace = await invoke<Workspace>('storage_load')
          applying = true
          codexPath.value = workspace.preferences.codexPath ?? ''
          settings.nativeLanguage = workspace.preferences.nativeLanguage
          settings.targetLanguage = workspace.preferences.targetLanguage
          mainModel.value = workspace.preferences.mainModel
          tutorModel.value = workspace.preferences.tutorModel
          tutorMode.value = workspace.preferences.tutorMode
          activeView.value = workspace.preferences.activeView
          mobilePane.value = workspace.preferences.mobilePane
          dbPath.value = workspace.path
          history.value = workspace.history
          applying = false
          adopt('main', workspace.main ?? (await create('main')))
          adopt('tutor', workspace.tutor ?? (await create('tutor')))
        } else {
          adopt('main', await create('main'))
          adopt('tutor', await create('tutor'))
        }
        initialized.value = true
        await backends.load()
        storageError.value = ''
        scheduleSave()
        await flush()
      } catch (e) {
        storageError.value = describe(e)
      } finally {
        applying = false
        loading.value = false
      }
    })()
    await loadingTask
    loadingTask = null
  }
  async function refreshHistory() {
    if (!isDesktop() || !initialized.value) return
    try {
      const request = ++historyRequest
      const result = await invoke<Conversation[]>('storage_list')
      if (request === historyRequest) history.value = result
    } catch (e) {
      storageError.value = describe(e)
    }
  }
  async function selectConversation(id: string, messageId?: string): Promise<boolean> {
    if (navigating.value || !initialized.value || closing.value) return false
    const entry = history.value.find((item) => item.id === id)
    navigationError.value = ''
    if (entry && lanes[entry.pane].busy) {
      navigationError.value = '请先停止该面板中的回复，再打开原会话。'
      return false
    }
    navigating.value = true
    try {
      if (!(await flush())) return false
      const conversation = await invoke<Conversation>('storage_read', { id })
      if (conversation.id !== id || !['main', 'tutor'].includes(conversation.pane))
        throw new Error('无法确认原会话，原句快照仍可查看。')
      if (messageId && !conversation.messages.some((message) => message.id === messageId))
        throw new Error('原消息已不存在，原句快照仍可查看。')
      if (lanes[conversation.pane].busy) throw new Error('请先停止该面板中的回复，再打开原会话。')
      if (closing.value) return false
      // Reading history can be slow; also save anything typed while it was loading.
      if (!(await flush())) return false
      adopt(conversation.pane, conversation, true)
      activeView.value = 'conversation'
      mobilePane.value = conversation.pane
      if (messageId)
        sourceFocus.value = {
          pane: conversation.pane,
          conversationId: id,
          messageId,
          request: ++sourceFocusRequest,
        }
      return await flush()
    } catch (e) {
      navigationError.value = describe(e)
      return false
    } finally {
      navigating.value = false
    }
  }
  async function deleteConversation(id: string) {
    const entry = history.value.find((item) => item.id === id)
    if (navigating.value || closing.value || !entry || lanes[entry.pane].busy) return
    navigating.value = true
    try {
      if (!(await flush())) return
      if (lanes[entry.pane].id === id) await resetLane(entry.pane)
      if (lanes[entry.pane].id === id) return
      await invoke('storage_delete', { id })
      await refreshHistory()
    } catch (e) {
      storageError.value = describe(e)
    } finally {
      navigating.value = false
    }
  }
  async function retryStorage() {
    if (!initialized.value) await initializeWorkspace()
    else {
      dirty = true
      await flush()
    }
  }
  function chooseDefaultModels() {
    for (const pane of ['main', 'tutor'] as const) {
      if (lanes[pane].backend.kind !== 'codex') continue
      const available = codexConnections.get(lanes[pane].backend.profileId).models
      const fallback = available.find((m) => m.isDefault)?.model ?? available[0]?.model ?? ''
      if (pane === 'main' && !mainModel.value) mainModel.value = fallback
      if (pane === 'tutor' && !tutorModel.value)
        tutorModel.value = available.find((m) => m.model.includes('luna'))?.model ?? fallback
    }
  }
  watch(
    () =>
      Object.values(lanes).map((lane) =>
        lane.backend.kind === 'codex' ? codexConnections.get(lane.backend.profileId).models : [],
      ),
    chooseDefaultModels,
    { flush: 'sync' },
  )
  async function refresh() {
    await connection.refresh()
    chooseDefaultModels()
  }
  function receive(event: ServerEvent) {
    const p = event.params
    const profileId = event.profileId ?? 'codex-default'
    if (event.method === 'storage/error') storageError.value = p.message ?? '保存失败'
    else if (event.method === 'connection/closed') {
      for (const lane of Object.values(lanes)) {
        if (lane.backend.kind !== 'codex' || lane.backend.profileId !== profileId) continue
        if (lane.busy) lane.error = '连接已中断，回复可能不完整。'
        lane.busy = false
        for (const message of lane.messages)
          if (message.status === 'pending' || message.status === 'streaming')
            message.status = 'interrupted'
        lane.request++
      }
    }
    if (!p.pane || !codexConnections.get(profileId).connected) return
    const lane = lanes[p.pane]
    if (
      lane.backend.kind !== 'codex' ||
      lane.backend.profileId !== profileId ||
      (event.profileRevision !== undefined &&
        event.profileRevision !== lane.backend.profileRevision) ||
      (p.conversationId && p.conversationId !== lane.id)
    )
      return
    if (event.method === 'item/agentMessage/delta' || event.method === 'item/completed') {
      const id = p.itemId ?? p.item?.id
      if (!id) return
      let message = lane.messages.find((m) => m.id === id)
      if (!message) {
        message = {
          id,
          role: 'assistant',
          text: '',
          status: 'streaming',
          vocabularyTarget: lane.vocabularyTarget,
        }
        lane.messages.push(message)
        message = lane.messages[lane.messages.length - 1]!
      }
      if (event.method === 'item/completed') {
        message.text = p.item?.text ?? message.text
        message.status = 'complete'
      } else if (message.status === 'streaming') message.text += p.delta ?? ''
    } else if (event.method === 'turn/started') {
      for (const message of lane.messages)
        if (message.status === 'pending') message.status = 'complete'
    } else if (event.method === 'turn/completed') {
      lane.busy = false
      for (const message of lane.messages)
        if (message.status === 'streaming' || message.status === 'pending')
          message.status =
            p.turn?.status === 'completed'
              ? 'complete'
              : p.turn?.status === 'interrupted'
                ? 'interrupted'
                : 'failed'
      void refreshHistory()
      if (p.turn?.error) lane.error = p.turn.error.message
      else if (p.turn?.status === 'interrupted') lane.error = '已停止回复。'
    } else if (event.method === 'error')
      lane.error = `${p.error?.message ?? '回复失败'}${p.willRetry ? '（Codex 正在重试）' : ''}`
  }
  onScopeDispose(connection.onEvent(receive))
  onScopeDispose(codexConnections.onEvent(receive))
  async function connect() {
    if (connecting.value) return
    if (!isDesktop()) {
      error.value = '请运行 npm run desktop:dev，在桌面应用中连接 Codex。'
      return
    }
    await initializeWorkspace()
    if (!initialized.value || storageError.value || !(await flush())) return
    await connection.connect(codexPath.value.trim())
    chooseDefaultModels()
  }
  async function connectCodexProfile(profileId: string) {
    if (profileId === 'codex-default') return connect()
    await initializeWorkspace()
    if (!initialized.value || closing.value || !(await flush())) return
    const profile = backends.profiles.find(
      (p) => p.id === profileId && p.config.kind === 'codex' && p.config.enabled,
    )
    if (!profile) return
    await codexConnections.get(profileId).connect(profile.config.binaryPath, profile.revision)
    chooseDefaultModels()
  }
  function codexScope(id: string) {
    return id === 'codex-default' ? {} : { profileId: id }
  }
  async function send(
    pane: Pane,
    text: string,
    mode = 'conversation',
    vocabularyTarget?: VocabularyAnswerTarget,
  ): Promise<boolean> {
    const lane = lanes[pane]
    const model = pane === 'main' ? mainModel.value : tutorModel.value
    const answerSnapshot = vocabularyTarget ? { ...vocabularyTarget } : undefined
    const terminalSnapshot = pane === 'tutor' ? terminalContext.value || null : null
    const targetLanguage = settings.targetLanguage
    const nativeLanguage = settings.nativeLanguage
    const initialId = lane.id
    const initialBackend = { ...lane.backend }
    if (!isReady(pane) || lane.busy || !text.trim() || !model) return false
    if (
      lane.backend.kind === 'codex' &&
      !codexConnections.get(lane.backend.profileId).models.some((m) => m.model === model)
    ) {
      lane.error = '当前账号不再提供已保存的模型，请在设置中选择可用模型。'
      return false
    }
    if (new TextEncoder().encode(text).length > 32000) {
      lane.error = '消息不能超过 32 KB。'
      return false
    }
    if (
      !(await flush()) ||
      lane.busy ||
      lane.id !== initialId ||
      lane.backend.profileId !== initialBackend.profileId ||
      lane.backend.profileRevision !== initialBackend.profileRevision ||
      !isReady(pane)
    )
      return false
    const signature = [model, targetLanguage, nativeLanguage, mode].join('|')
    if (lane.signature && lane.signature !== signature) {
      const draft = lane.draft
      await reset(pane)
      if (lane.signature) return false
      lane.draft = draft
      if (!(await flush())) return false
    }
    if (lane.busy || !isReady(pane)) return false
    if (sourceFocus.value?.pane === pane) sourceFocus.value = null
    lane.vocabularyTarget = answerSnapshot
    lane.signature = signature
    lane.error = ''
    lane.notice = undefined
    lane.busy = true
    const request = ++lane.request
    const messageId = crypto.randomUUID()
    const conversationId = lane.id
    const backend = { ...lane.backend }
    lane.apiRequestId = messageId
    lane.messages.push({ id: messageId, role: 'user', text: text.trim(), status: 'pending' })
    try {
      const payload = {
        pane,
        conversationId,
        messageId,
        text: text.trim(),
        model,
        targetLanguage,
        nativeLanguage,
        terminalContext: terminalSnapshot,
        mode,
      }
      if (backend.kind === 'codex')
        await invoke('codex_send', { request: payload, ...codexScope(backend.profileId) })
      else {
        let sequence = 0
        let finished = false
        const events = new Channel<TurnEvent>()
        events.onmessage = (event) => {
          if (
            finished ||
            request !== lane.request ||
            lane.id !== conversationId ||
            event.pane !== pane ||
            event.conversationId !== conversationId ||
            event.requestId !== messageId ||
            event.profileId !== backend.profileId ||
            event.profileRevision !== backend.profileRevision ||
            event.sequence <= sequence
          )
            return
          sequence = event.sequence
          let answer = lane.messages.find((m) => m.id === event.messageId)
          if (!answer) {
            lane.messages.push({
              id: event.messageId,
              role: 'assistant',
              text: '',
              status: 'streaming',
              vocabularyTarget: answerSnapshot,
            })
            answer = lane.messages.at(-1)!
          }
          answer.text = event.text
          answer.status = event.status
          if (event.usage) answer.usage = event.usage
          const user = lane.messages.find((m) => m.id === messageId)
          if (user) user.status = 'complete'
          if (event.error) lane.error = event.error
          if (event.notice) lane.notice = event.notice
          if (event.status !== 'streaming') {
            finished = true
            lane.busy = false
            lane.apiRequestId = undefined
            void refreshHistory()
          }
        }
        await invoke('backend_send', {
          request: {
            ...payload,
            profileId: backend.profileId,
            profileRevision: backend.profileRevision,
          },
          events,
        })
      }
      if (request === lane.request && lane.draft === text) lane.draft = ''
      await flush()
      await refreshHistory()
      return true
    } catch (e) {
      if (request === lane.request) {
        lane.error = describe(e)
        lane.busy = false
        lane.apiRequestId = undefined
        const message = lane.messages.find((m) => m.id === messageId)
        if (message) message.status = 'failed'
        // Keep the draft for an explicit retry. Never resend automatically.
        scheduleSave()
        await flush()
        await refreshHistory()
      }
      return false
    }
  }
  async function stop(pane: Pane) {
    try {
      const lane = lanes[pane]
      if (lane.backend.kind === 'codex')
        await invoke('codex_stop', { pane, ...codexScope(lane.backend.profileId) })
      else
        await invoke('backend_stop', {
          conversationId: lane.id,
          requestId: lane.apiRequestId ?? null,
        })
    } catch (e) {
      lanes[pane].error = describe(e)
    }
  }
  async function resetLane(pane: Pane) {
    if (
      lanes[pane].backend.kind === 'codex' &&
      codexConnections.get(lanes[pane].backend.profileId).connected
    )
      await invoke('codex_reset', { pane, ...codexScope(lanes[pane].backend.profileId) })
    const conversation = await create(pane)
    if (!(await flush()) || closing.value) return
    adopt(pane, conversation)
    await flush()
    await refreshHistory()
  }
  async function reset(pane: Pane) {
    if (navigating.value || closing.value || lanes[pane].busy || !initialized.value) return
    navigating.value = true
    try {
      if (await flush()) await resetLane(pane)
    } catch (e) {
      storageError.value = describe(e)
    } finally {
      navigating.value = false
    }
  }
  async function selectBackend(pane: Pane, profileId: string) {
    if (navigating.value || closing.value || !initialized.value || lanes[pane].busy) return
    const profile = backends.profiles.find((p) => p.id === profileId && p.config.enabled)
    if (!profile) return
    if (
      profileId === lanes[pane].backend.profileId &&
      profile.revision === lanes[pane].backend.profileRevision
    )
      return
    navigating.value = true
    try {
      if (!(await flush())) return
      const changing = profileId !== lanes[pane].backend.profileId
      const conversation = await create(pane, profileId)
      if (!(await flush()) || closing.value) return
      const draft = lanes[pane].draft
      adopt(pane, conversation)
      lanes[pane].draft = draft
      if (changing) {
        const fallback =
          profile.config.kind === 'codex'
            ? (codexConnections.get(profileId).models.find((m) => m.isDefault)?.model ??
              codexConnections.get(profileId).models[0]?.model ??
              '')
            : (backends.state(profileId).models[0] ?? '')
        if (pane === 'main') mainModel.value = fallback
        else tutorModel.value = fallback
      }
      await flush()
      await refreshHistory()
    } catch (e) {
      lanes[pane].error = describe(e)
    } finally {
      navigating.value = false
    }
  }
  return {
    initialized,
    loading,
    storageError,
    dbPath,
    history,
    tutorMode,
    activeView,
    mobilePane,
    saving,
    closing,
    initializeWorkspace,
    flush,
    refreshHistory,
    selectConversation,
    navigating,
    navigationError,
    sourceFocus,
    deleteConversation,
    retryStorage,
    connected,
    connecting,
    error,
    notice,
    account,
    accountChecked,
    checkingAccount,
    loadingModels,
    needsLogin,
    limitsError,
    models,
    codexPath,
    terminalContext,
    mainModel,
    tutorModel,
    limits,
    login,
    loggingIn,
    lanes,
    ready,
    isReady,
    paneLabel,
    selectBackend,
    label,
    connect,
    connectCodexProfile,
    disconnect,
    refresh,
    signIn,
    openLogin,
    cancelLogin,
    send,
    stop,
    reset,
  }
})
