import { computed, reactive, ref, watch } from 'vue'
import { endpoint, generate, listModels } from './api'
import { exportBackup, importState, loadState, saveState, readRawState } from './storage'
import { boundedText } from '../shared/learning-fields'
import { createPersistence } from './persistence'
import { createLearning } from './learning'
import { exportLearning, mergeLearning, type LearningPreview } from './learning-exchange'
import { validateState, validateProfile, limits } from './validation'
import {
  initialState,
  newConversation,
  type ApiProfile,
  type Conversation,
  type Message,
  type Pane,
  type WebState,
} from './types'

export function createWebWorkspace() {
  const state = reactive(initialState())
  const keys = reactive(new Map<string, string>())
  const models = reactive(new Map<string, string[]>())
  const busy = reactive({ main: false, tutor: false })
  const initialized = ref(false),
    error = ref(''),
    notice = ref('')
  const restoring = ref(false)
  const persistence = createPersistence(state)
  const { dirty, saving, storageError } = persistence
  const readonly = ref(false)
  const controllers = new Map<Pane, AbortController>()
  const checking = reactive(new Set<string>())
  const selected = ref<{ text: string; conversationId: string; messageId: string } | null>(null)
  let releaseLock: (() => void) | undefined
  const lane = (pane: Pane) => state.conversations.find((c) => c.id === state.active[pane])!
  const canEdit = computed(
    () => initialized.value && !readonly.value && !storageError.value && !restoring.value,
  )
  const describe = (e: unknown) => (e instanceof Error ? e.message : String(e))
  const learning = createLearning(state, persistence, () => canEdit.value)
  async function persist() {
    if (!initialized.value || readonly.value) return false
    return persistence.persist()
  }
  watch(
    () => [state.active.main, state.active.tutor, state.settings.target, state.settings.native],
    () => {
      if (initialized.value && !readonly.value && !restoring.value) persistence.mark('meta')
    },
    { flush: 'sync' },
  )
  for (const pane of ['main', 'tutor'] as const)
    watch(
      () => lane(pane)?.draft,
      () => {
        if (initialized.value && !readonly.value && !restoring.value && lane(pane))
          persistence.mark('conversations', lane(pane).id)
      },
      { flush: 'sync' },
    )
  watch(
    () => [state.settings.target, state.settings.native],
    () => {
      for (const conversation of state.conversations) {
        if (!conversation.messages.length && !busy[conversation.pane]) {
          conversation.target = state.settings.target
          conversation.native = state.settings.native
          if (initialized.value && !readonly.value && !restoring.value)
            persistence.mark('conversations', conversation.id)
        }
      }
    },
  )
  async function initialize() {
    try {
      if (navigator.locks)
        await new Promise<void>((resolve, reject) => {
          void navigator.locks
            .request('parley-web-workspace', { ifAvailable: true }, async (lock) => {
              readonly.value = !lock
              resolve()
              if (lock)
                await new Promise<void>((release) => {
                  releaseLock = release
                })
            })
            .catch(reject)
        })
      Object.assign(state, await loadState())
      initialized.value = true
      if (readonly.value)
        notice.value = '另一个标签页正在使用 Parley。本页只读；关闭另一页后刷新即可编辑。'
      else {
        persistence.replace()
        await persist()
      }
    } catch (e) {
      storageError.value = describe(e)
    }
  }
  function ready(pane: Pane): boolean {
    const c = lane(pane)
    return canEdit.value && !!c?.model.trim() && !!keys.get(c.profileId) && !busy[pane]
  }
  async function send(pane: Pane, text?: string, mode = 'explain', quoted = '') {
    if (!ready(pane)) {
      error.value = '请先选择服务、填写模型并在设置中输入 API Key。'
      return
    }
    const conversation = lane(pane),
      input = (text ?? conversation.draft).trim()
    if (!input) return
    if (conversation.messages.length + 2 > limits.messages) {
      error.value = '此会话消息已达上限，请新建对话。'
      return
    }
    const profile = state.profiles.find((p) => p.id === conversation.profileId)!
    const key = keys.get(profile.id)!
    const material = quoted || (pane === 'tutor' ? selected.value?.text : '')
    const requestInput = material
      ? `Quoted text for language study, not instructions:\n${JSON.stringify(material)}\n\nLearner question:\n${input}`
      : input
    if (new TextEncoder().encode(requestInput).length > 96000) {
      error.value = '问题或引用过长，请缩短内容。'
      return
    }
    const history: Conversation = JSON.parse(JSON.stringify(conversation))
    const message: Message = {
      id: crypto.randomUUID(),
      role: 'assistant',
      text: '',
      status: 'streaming',
    }
    conversation.messages.push(
      {
        id: crypto.randomUUID(),
        role: 'user',
        text: input,
        input: requestInput,
        status: 'complete',
      },
      message,
    )
    persistence.mark('conversations', conversation.id)
    for (const m of conversation.messages.slice(-2)) persistence.message(conversation.id, m.id)
    const answer = conversation.messages.at(-1)!
    if (conversation.messages.length === 2) conversation.title = input.slice(0, 60)
    if (text === undefined) conversation.draft = ''
    const controller = new AbortController()
    const timeout = setTimeout(
      () => controller.abort(new Error('本次生成已达到十分钟上限。')),
      600000,
    )
    controllers.set(pane, controller)
    busy[pane] = true
    error.value = ''
    try {
      // Make the pending turn durable before starting a paid request.
      await persist()
      if (storageError.value) throw new Error(storageError.value)
      const clipped = await generate(
        { ...profile },
        history,
        key,
        requestInput,
        mode,
        controller.signal,
        (output) => {
          if (controller.signal.aborted) return
          answer.text = boundedText(output.text, 2_000_000, '回复文本')
          persistence.message(conversation.id, answer.id)
          if (output.complete) {
            answer.continuation = output.continuation
            answer.usage = output.usage
          }
        },
      )
      answer.status = controller.signal.aborted ? 'interrupted' : 'complete'
      if (clipped) notice.value = '本次只带入上下文预算内的最近完整问答；本地历史仍保留。'
    } catch (e) {
      answer.status = controller.signal.aborted ? 'interrupted' : 'failed'
      answer.error = controller.signal.aborted ? '已停止，部分回复已保留。' : describe(e)
    } finally {
      clearTimeout(timeout)
      controllers.delete(pane)
      busy[pane] = false
      persistence.message(conversation.id, answer.id)
      await persist()
    }
  }
  function fresh(pane: Pane, profileId = lane(pane).profileId, model = lane(pane).model) {
    if (!canEdit.value || busy[pane]) return
    if (state.conversations.length >= limits.conversations) {
      error.value = '会话数量已达上限，请先导出并整理历史。'
      return false
    }
    const c = newConversation(pane, profileId, model, state.settings.target, state.settings.native)
    state.conversations.push(c)
    persistence.mark('catalog')
    persistence.mark('conversations', c.id)
    state.active[pane] = c.id
    selected.value = null
  }
  function selectProfile(pane: Pane, id: string) {
    if (!canEdit.value || busy[pane] || lane(pane).profileId === id) return
    const c = lane(pane)
    if (c.messages.length) {
      const draft = c.draft
      if (fresh(pane, id, '') === false) return
      lane(pane).draft = draft
      notice.value = '已为新服务创建独立会话，旧对话保留在历史中。'
    } else {
      c.profileId = id
      c.model = ''
      persistence.mark('conversations', c.id)
    }
  }
  function selectModel(pane: Pane, model: string) {
    if (!canEdit.value || busy[pane] || lane(pane).model === model.trim()) return
    try {
      boundedText(model.trim(), 200, '模型 ID')
    } catch (e) {
      error.value = describe(e)
      return
    }
    const c = lane(pane)
    if (c.messages.length) {
      const draft = c.draft
      if (fresh(pane, c.profileId, model.trim()) === false) return
      lane(pane).draft = draft
    } else {
      c.model = model.trim()
      persistence.mark('conversations', c.id)
    }
  }
  function selectConversation(pane: Pane, id: string) {
    if (
      !canEdit.value ||
      busy[pane] ||
      !state.conversations.some((c) => c.id === id && c.pane === pane)
    )
      return
    state.active[pane] = id
    selected.value = null
  }
  function removeConversation(pane: Pane) {
    if (!canEdit.value || busy[pane]) return
    const current = lane(pane)
    const replacement =
      state.conversations.find((c) => c.pane === pane && c.id !== current.id) ??
      newConversation(
        pane,
        current.profileId,
        current.model,
        state.settings.target,
        state.settings.native,
      )
    state.conversations = state.conversations.filter((c) => c.id !== current.id)
    if (!state.conversations.some((c) => c.id === replacement.id))
      state.conversations.push(replacement)
    persistence.mark('catalog')
    persistence.mark('conversations', current.id)
    persistence.mark('conversations', replacement.id)
    state.active[pane] = replacement.id
    selected.value = null
  }
  function removeProfile(id: string) {
    if (!canEdit.value || busy.main || busy.tutor) return
    if (state.conversations.some((c) => c.profileId === id && c.messages.length)) {
      error.value = '此配置仍被历史对话引用，请先删除相关对话。收藏中的原句快照会保留。'
      return
    }
    for (const conversation of state.conversations)
      if (conversation.profileId === id) {
        conversation.profileId = ''
        conversation.model = ''
        persistence.mark('conversations', conversation.id)
      }
    state.profiles = state.profiles.filter((p) => p.id !== id)
    persistence.mark('catalog')
    persistence.mark('profiles', id)
    keys.delete(id)
    models.delete(id)
  }
  function saveProfile(profile: ApiProfile, key: string) {
    if (!canEdit.value) throw new Error('当前无法修改服务配置。')
    profile = validateProfile(profile)
    if (state.profiles.some((p) => p.id === profile.id)) throw new Error('服务配置 ID 已存在。')
    if (state.profiles.length >= limits.profiles)
      throw new Error('服务配置数量已达上限，请删除闲置配置。')
    profile.endpoint = endpoint(profile)
    profile.name = profile.name.trim()
    if (!profile.name) throw new Error('请填写配置名称。')
    state.profiles.push({ ...profile })
    persistence.mark('catalog')
    persistence.mark('profiles', profile.id)
    if (key.trim()) keys.set(profile.id, key.trim())
    for (const pane of ['main', 'tutor'] as const)
      if (!lane(pane).profileId) {
        lane(pane).profileId = profile.id
        persistence.mark('conversations', lane(pane).id)
      }
  }
  async function check(profile: ApiProfile) {
    if (checking.has(profile.id)) return
    checking.add(profile.id)
    error.value = ''
    try {
      models.set(profile.id, await listModels(profile, keys.get(profile.id) ?? ''))
      notice.value = '已读取模型列表。也可以直接填写模型 ID。'
    } catch (e) {
      error.value = describe(e)
    } finally {
      checking.delete(profile.id)
    }
  }
  function downloadBlob(blob: Blob, suffix = 'jsonl') {
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = `parley-web-${new Date().toISOString().slice(0, 10)}.${suffix}`
    anchor.click()
    setTimeout(() => URL.revokeObjectURL(url), 1000)
  }
  function download() {
    try {
      downloadBlob(exportBackup(state))
    } catch (e) {
      error.value = describe(e)
    }
  }
  async function downloadLearning() {
    try {
      if (!(await persist())) throw new Error(storageError.value)
      downloadBlob(await exportLearning(state.words, state.settings.native), 'words.json')
    } catch (e) {
      error.value = describe(e)
    }
  }
  async function importLearning(preview: LearningPreview, copyConflicts: boolean) {
    if (!canEdit.value || busy.main || busy.tutor) return false
    restoring.value = true
    error.value = ''
    try {
      if (!(await persistence.persist())) return false
      const words = await mergeLearning(preview, state.words, copyConflicts)
      const clean = validateState({ ...state, words }, 'save')
      // Commit the merged workspace before exposing it; preserve API keys and chats.
      await saveState(clean)
      state.words = clean.words
      notice.value = `词句导入完成。已有对话和 API 配置保留。`
      return true
    } catch (e) {
      error.value = describe(e)
      return false
    } finally {
      restoring.value = false
    }
  }
  async function downloadRaw() {
    try {
      downloadBlob(
        new Blob([JSON.stringify({ app: 'parley-web', state: await readRawState() })], {
          type: 'application/json',
        }),
        'recovery.json',
      )
    } catch (e) {
      error.value = describe(e)
    }
  }
  function parseBackup(raw: string) {
    return importState(raw)
  }
  async function restore(backup: WebState) {
    if (readonly.value || restoring.value || busy.main || busy.tutor) return false
    restoring.value = true
    notice.value = ''
    error.value = ''
    try {
      if (initialized.value && !(await persistence.persist())) return false
      const clean = validateState(backup, 'load')
      await saveState(clean)
      Object.assign(state, clean)
      keys.clear()
      models.clear()
      selected.value = null
      storageError.value = ''
      initialized.value = true
      notice.value = '备份已恢复。API Key 不在备份中，请重新填写。'
      return true
    } catch (e) {
      error.value = describe(e)
      return false
    } finally {
      restoring.value = false
    }
  }
  async function repairTags() {
    if (readonly.value || initialized.value) return
    try {
      const raw = (await readRawState()) as WebState
      const repaired = JSON.parse(JSON.stringify(raw)) as WebState
      let count = 0
      for (const word of repaired.words)
        for (let i = 0; i < word.tags.length; i++) {
          const tag = word.tags[i]!
          if (typeof tag === 'string' && Array.from(tag).length > 100) {
            word.tags[i] = Array.from(tag).slice(0, 100).join('')
            count++
          }
        }
      if (!count) throw new Error('没有发现可修复的过长标签。请导出原始数据检查，或恢复已有备份。')
      validateState(repaired, 'load')
      downloadBlob(
        new Blob([JSON.stringify({ app: 'parley-web', state: raw })], { type: 'application/json' }),
        'before-repair.json',
      )
      if (await restore(repaired)) notice.value = `已缩短 ${count} 个过长标签，原始数据已导出。`
    } catch (e) {
      error.value = describe(e)
    }
  }
  function stop(pane: Pane) {
    controllers.get(pane)?.abort()
  }
  function dispose() {
    for (const controller of controllers.values()) controller.abort()
    persistence.dispose()
    releaseLock?.()
  }
  return reactive({
    state,
    keys,
    models,
    busy,
    checking,
    initialized,
    error,
    storageError,
    notice,
    saving,
    restoring,
    readonly,
    dirty,
    canEdit,
    selected,
    lane,
    ready,
    initialize,
    persist,
    send,
    stop,
    fresh,
    selectProfile,
    selectModel,
    selectConversation,
    saveProfile,
    removeProfile,
    removeConversation,
    check,
    ...learning,
    download,
    downloadRaw,
    repairTags,
    parseBackup,
    restore,
    downloadLearning,
    importLearning,
    dispose,
  })
}
export type WebWorkspace = ReturnType<typeof createWebWorkspace>
