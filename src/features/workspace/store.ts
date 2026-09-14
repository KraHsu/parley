import { computed, reactive, ref, watch } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from '../../shared/desktop'
import { useSettingsStore } from '../settings/store'
import type { Conversation, Pane } from '../chat/types'
import type { Workspace } from './types'

type Document = Pick<
  Conversation,
  'id' | 'draft' | 'title' | 'messages' | 'signature' | 'status'
> & {
  context: NonNullable<Conversation['context']>
  backend: NonNullable<Conversation['backend']>
}
const emptyDocument = (): Document => ({
  id: '',
  draft: '',
  title: '新的对话',
  messages: [],
  context: [],
  signature: '',
  status: 'idle',
  backend: { profileId: 'codex-default', profileRevision: 1, kind: 'codex' },
})
const describe = (error: unknown) => (error instanceof Error ? error.message : String(error))

// Local documents and write ordering are independent of model connections.
export const useWorkspaceStore = defineStore('workspace', () => {
  const settings = useSettingsStore()
  const codexPath = ref('')
  const mainModel = ref('')
  const tutorModel = ref('')
  const documents = reactive<Record<Pane, Document>>({
    main: emptyDocument(),
    tutor: emptyDocument(),
  })
  const selectionVersion = reactive<Record<Pane, number>>({ main: 0, tutor: 0 })
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
  const ready = computed(() => initialized.value && !storageError.value && !closing.value)
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
        mainId: documents.main.id || null,
        tutorId: documents.tutor.id || null,
        tutorMode: tutorMode.value,
        activeView: activeView.value,
        mobilePane: mobilePane.value,
      },
      drafts: Object.values(documents)
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
      documents.main.id,
      documents.main.draft,
      documents.tutor.id,
      documents.tutor.draft,
    ],
    scheduleSave,
    { flush: 'sync' },
  )
  function adopt(pane: Pane, conversation: Conversation, restoreSettings = false) {
    applying = true
    Object.assign(documents[pane], {
      id: conversation.id,
      title: conversation.title,
      draft: conversation.draft,
      messages: conversation.messages,
      context: conversation.context ?? [],
      signature: conversation.signature,
      backend: conversation.backend ?? emptyDocument().backend,
      status: conversation.status,
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
    selectionVersion[pane]++
    applying = false
    scheduleSave()
  }
  async function create(
    pane: Pane,
    profileId = documents[pane].backend.profileId,
    sourceId?: string,
  ) {
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
    return invoke<Conversation>('storage_create', {
      id,
      pane,
      profileId,
      ...(sourceId ? { sourceId } : {}),
    })
  }
  async function initialize() {
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
    const request = ++historyRequest
    try {
      const result = await invoke<Conversation[]>('storage_list')
      if (request === historyRequest) history.value = result
    } catch (e) {
      if (request === historyRequest) storageError.value = describe(e)
    }
  }
  async function retryStorage() {
    if (!initialized.value) await initialize()
    else {
      dirty = true
      await flush()
    }
  }
  function read(id: string) {
    return invoke<Conversation>('storage_read', { id })
  }
  function remove(id: string) {
    return invoke('storage_delete', { id })
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
    ready,
    codexPath,
    mainModel,
    tutorModel,
    documents,
    selectionVersion,
    initialize,
    flush,
    refreshHistory,
    retryStorage,
    scheduleSave,
    adopt,
    create,
    read,
    remove,
  }
})
