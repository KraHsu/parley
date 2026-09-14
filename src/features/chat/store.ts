import { computed, reactive, ref, watch, onScopeDispose } from 'vue'
import { defineStore, storeToRefs } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from '../../shared/desktop'
import { useSettingsStore } from '../settings/store'

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
  const connection = useCodexConnectionStore()
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
  const saving = ref(false)
  const closing = ref(false)
  const ready = computed(
    () =>
      connected.value &&
      account.value?.type === 'chatgpt' &&
      models.value.length > 0 &&
      initialized.value &&
      !storageError.value &&
      !closing.value,
  )
  function isReady(pane: Pane) {
    return ready.value && lanes[pane].backend.profileId === 'codex-default'
  }
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
  async function create(pane: Pane) {
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
    return invoke<Conversation>('storage_create', { id, pane })
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
  async function selectConversation(id: string) {
    const entry = history.value.find((item) => item.id === id)
    if (!entry || lanes[entry.pane].busy || !(await flush())) return
    try {
      adopt(entry.pane, await invoke<Conversation>('storage_read', { id }), true)
      activeView.value = 'conversation'
      mobilePane.value = entry.pane
      await flush()
    } catch (e) {
      storageError.value = describe(e)
    }
  }
  async function deleteConversation(id: string) {
    const entry = history.value.find((item) => item.id === id)
    if (!entry || lanes[entry.pane].busy || !(await flush())) return
    try {
      if (lanes[entry.pane].id === id) await reset(entry.pane)
      if (lanes[entry.pane].id === id) return
      await invoke('storage_delete', { id })
      await refreshHistory()
    } catch (e) {
      storageError.value = describe(e)
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
    const available = models.value
    const fallback = available.find((m) => m.isDefault)?.model ?? available[0]?.model ?? ''
    if (!mainModel.value) mainModel.value = fallback
    if (!tutorModel.value)
      tutorModel.value = available.find((m) => m.model.includes('luna'))?.model ?? fallback
  }
  watch(models, chooseDefaultModels, { flush: 'sync' })
  async function refresh() {
    await connection.refresh()
    chooseDefaultModels()
  }
  function receive(event: ServerEvent) {
    const p = event.params
    if (event.method === 'storage/error') storageError.value = p.message ?? '保存失败'
    else if (event.method === 'connection/closed') {
      for (const lane of Object.values(lanes)) {
        if (lane.backend.profileId !== 'codex-default') continue
        if (lane.busy) lane.error = '连接已中断，回复可能不完整。'
        lane.busy = false
        for (const message of lane.messages)
          if (message.status === 'pending' || message.status === 'streaming')
            message.status = 'interrupted'
        lane.request++
      }
    }
    if (!p.pane || !connected.value) return
    const lane = lanes[p.pane]
    if (
      lane.backend.profileId !== 'codex-default' ||
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
    if (!isReady(pane) || lane.busy || !text.trim() || !model) return false
    if (!models.value.some((m) => m.model === model)) {
      lane.error = '当前账号不再提供已保存的模型，请在设置中选择可用模型。'
      return false
    }
    if (new TextEncoder().encode(text).length > 32000) {
      lane.error = '消息不能超过 32 KB。'
      return false
    }
    if (!(await flush()) || lane.busy) return false
    const signature = [model, settings.targetLanguage, settings.nativeLanguage, mode].join('|')
    if (lane.signature && lane.signature !== signature) {
      const draft = lane.draft
      await reset(pane)
      if (lane.signature) return false
      lane.draft = draft
      if (!(await flush())) return false
    }
    if (lane.busy) return false
    lane.vocabularyTarget = answerSnapshot
    lane.signature = signature
    lane.error = ''
    lane.busy = true
    const request = ++lane.request
    const messageId = crypto.randomUUID()
    const conversationId = lane.id
    lane.messages.push({ id: messageId, role: 'user', text: text.trim(), status: 'pending' })
    try {
      await invoke('codex_send', {
        request: {
          pane,
          conversationId,
          messageId,
          text: text.trim(),
          model,
          targetLanguage: settings.targetLanguage,
          nativeLanguage: settings.nativeLanguage,
          terminalContext: terminalSnapshot,
          mode,
        },
      })
      if (request === lane.request && lane.draft === text) lane.draft = ''
      await flush()
      await refreshHistory()
      return true
    } catch (e) {
      if (request === lane.request) {
        lane.error = describe(e)
        lane.busy = false
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
      await invoke('codex_stop', { pane })
    } catch (e) {
      lanes[pane].error = describe(e)
    }
  }
  async function reset(pane: Pane) {
    if (lanes[pane].busy || !initialized.value || !(await flush())) return
    try {
      if (connected.value) await invoke('codex_reset', { pane })
      adopt(pane, await create(pane))
      await flush()
      await refreshHistory()
    } catch (e) {
      storageError.value = describe(e)
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
    label,
    connect,
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
