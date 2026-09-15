import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { Channel, invoke } from '@tauri-apps/api/core'
import { isDesktop } from '../../shared/desktop'
import type { Pane } from '../chat/types'

interface Model {
  id: string
  model: string
  displayName: string
  isDefault: boolean
}
interface Account {
  type: string
  email?: string | null
  planType?: string
}
interface LimitWindow {
  usedPercent: number
  windowDurationMins: number | null
  resetsAt: number | null
}
interface Limit {
  limitName?: string | null
  primary?: LimitWindow | null
  secondary?: LimitWindow | null
}
interface Status {
  account: Account | null
  models: Model[]
  limits: { rateLimits?: Limit; rateLimitsByLimitId?: Record<string, Limit> } | null
}
export interface ServerEvent {
  profileId?: string
  profileRevision?: number
  method: string
  params: {
    pane?: Pane
    conversationId?: string
    message?: string
    delta?: string
    itemId?: string
    item?: { id: string; text: string }
    turn?: { status: string; error?: { message: string } | null }
    error?: { message: string }
    willRetry?: boolean
    success?: boolean
    loginId?: string
    rateLimits?: Limit
  }
}

const describe = (error: unknown) => (error instanceof Error ? error.message : String(error))

export function createCodexConnection(profileId = 'codex-default') {
  const profileRevision = ref<number | null>(null)
  const scope = profileId === 'codex-default' ? {} : { profileId }
  function rpc<T>(method: string, args: Record<string, unknown> = {}): Promise<T> {
    const scoped = { ...args, ...scope }
    return Object.keys(scoped).length ? invoke<T>(method, scoped) : invoke<T>(method)
  }
  const connected = ref(false)
  const connecting = ref(false)
  const error = ref('')
  const notice = ref('')
  const account = ref<Account | null>(null)
  const accountChecked = ref(false)
  const checkingAccount = ref(false)
  const loadingModels = ref(false)
  const limitsError = ref('')
  const needsLogin = computed(
    () =>
      connected.value &&
      accountChecked.value &&
      !checkingAccount.value &&
      account.value?.type !== 'chatgpt',
  )
  const models = ref<Model[]>([])
  const limits = ref<Status['limits']>(null)
  const login = ref<{ loginId: string; authUrl: string } | null>(null)
  const loggingIn = ref(false)
  const listeners = new Set<(event: ServerEvent) => void>()
  function onEvent(listener: (event: ServerEvent) => void) {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }
  const label = computed(() =>
    connecting.value
      ? '连接中…'
      : checkingAccount.value
        ? '读取本机账号…'
        : !connected.value
          ? '未连接'
          : account.value?.type === 'chatgpt'
            ? loadingModels.value
              ? '已登录 · 读取模型…'
              : '已登录'
            : accountChecked.value
              ? '待登录'
              : '账号状态未知',
  )
  let epoch = 0
  let loginAttempt = 0
  let statusRequest = 0

  async function refresh() {
    if (!connected.value) return
    const current = epoch
    const request = ++statusRequest
    const valid = () => current === epoch && request === statusRequest && connected.value
    checkingAccount.value = true
    accountChecked.value = false
    loadingModels.value = false
    try {
      const status = await rpc<Pick<Status, 'account'>>('codex_status', { section: 'account' })
      if (!valid()) return
      account.value = status.account
      accountChecked.value = true
      checkingAccount.value = false
      error.value = ''
      if (status.account?.type !== 'chatgpt') {
        models.value = []
        limits.value = null
        return
      }
      // Existing CLI credentials are sufficient; never start another browser login.
      if (loggingIn.value || login.value) loginAttempt++
      loggingIn.value = false
      login.value = null
      loadingModels.value = true
      limitsError.value = ''
      void rpc<Pick<Status, 'limits'>>('codex_status', { section: 'limits' })
        .then((result) => {
          if (valid()) limits.value = result.limits
        })
        .catch(() => {
          if (valid()) {
            limits.value = null
            limitsError.value = '额度暂时无法读取，不影响登录。可点击刷新重试。'
          }
        })
      try {
        const result = await rpc<Pick<Status, 'models'>>('codex_status', { section: 'models' })
        if (!valid()) return
        models.value = result.models
        if (!result.models.length) error.value = '本机账号已登录，但没有返回可用模型。请刷新重试。'
      } catch (e) {
        if (valid()) error.value = `本机账号已登录，模型列表读取失败：${describe(e)}`
      }
    } catch (e) {
      if (valid()) error.value = `无法读取本机登录状态：${describe(e)}`
    } finally {
      if (valid()) {
        checkingAccount.value = false
        loadingModels.value = false
      }
    }
  }
  function receive(event: ServerEvent) {
    if (event.profileId && event.profileId !== profileId) return
    if (
      event.profileRevision &&
      profileRevision.value &&
      event.profileRevision !== profileRevision.value
    )
      return
    event = { ...event, profileId }
    const p = event.params
    if (event.method === 'connection/closed') {
      epoch++
      loginAttempt++
      connecting.value = false
      connected.value = false
      account.value = null
      accountChecked.value = false
      checkingAccount.value = false
      loadingModels.value = false
      models.value = []
      limits.value = null
      limitsError.value = ''
      login.value = null
      loggingIn.value = false
      error.value = p.message ?? 'Codex 已断开'
    } else if (event.method === 'connection/notice') notice.value = p.message ?? ''
    else if (event.method === 'account/login/completed') {
      loginAttempt++
      login.value = null
      loggingIn.value = false
      if (p.success) {
        error.value = ''
        void refresh()
      } else error.value = typeof p.error === 'string' ? p.error : '登录未完成，请重试。'
    } else if (event.method === 'account/updated') void refresh()
    else if (event.method === 'account/rateLimits/updated' && p.rateLimits)
      limits.value = { ...limits.value, rateLimits: p.rateLimits }
    for (const listener of listeners) listener(event)
  }
  async function connect(path: string, revision?: number) {
    if (connecting.value) return
    if (!isDesktop()) {
      error.value = '请运行 npm run desktop:dev，在桌面应用中连接 Codex。'
      return
    }
    if (!path) {
      error.value = '请在设置中填写你自己的 Codex 可执行文件完整路径。'
      return
    }
    const current = ++epoch
    profileRevision.value = revision ?? null
    connecting.value = true
    connected.value = false
    error.value = ''
    notice.value = ''
    const events = new Channel<ServerEvent>()
    events.onmessage = (event) => {
      if (current === epoch) receive(event)
    }
    try {
      const result = await rpc<{ profileRevision?: number } | null>('codex_connect', {
        events,
        codexPath: path,
        ...(revision === undefined ? {} : { expectedRevision: revision }),
      })
      if (current === epoch) profileRevision.value = result?.profileRevision ?? revision ?? null
      if (current !== epoch) return
      connected.value = true
      connecting.value = false
      await refresh()
    } catch (e) {
      if (current === epoch) error.value = describe(e)
    } finally {
      if (current === epoch) connecting.value = false
    }
  }
  async function disconnect() {
    receive({
      method: 'connection/closed',
      profileId,
      params: { message: '已断开 Codex，本机账号登录状态保留。' },
    })
    const current = epoch
    try {
      await rpc('codex_disconnect', { profileId })
    } catch (e) {
      if (current === epoch) error.value = describe(e)
    }
  }
  async function openLogin() {
    if (!login.value) return
    const current = epoch
    const pending = login.value
    try {
      await rpc('codex_open_login', { url: pending.authUrl })
    } catch (e) {
      if (current === epoch && login.value === pending)
        error.value = `无法打开浏览器：${describe(e)}`
    }
  }
  async function signIn() {
    if (loggingIn.value) return
    const current = epoch
    const attempt = ++loginAttempt
    loggingIn.value = true
    error.value = ''
    try {
      await refresh()
      if (current !== epoch || attempt !== loginAttempt) return
      if (!needsLogin.value) {
        loggingIn.value = false
        return
      }
      const result = await rpc<{ loginId: string; authUrl: string }>('codex_login')
      if (current !== epoch || attempt !== loginAttempt) return
      login.value = result
      await openLogin()
    } catch (e) {
      if (current === epoch && attempt === loginAttempt) {
        error.value = describe(e)
        loggingIn.value = false
      }
    }
  }
  async function cancelLogin() {
    if (!login.value) return
    const current = epoch
    const attempt = ++loginAttempt
    const pending = login.value
    try {
      await rpc('codex_cancel_login', { loginId: pending.loginId })
      if (current === epoch && attempt === loginAttempt) {
        login.value = null
        loggingIn.value = false
      }
    } catch (e) {
      if (current === epoch && attempt === loginAttempt) error.value = describe(e)
    }
  }
  const ready = computed(
    () => connected.value && account.value?.type === 'chatgpt' && models.value.length > 0,
  )
  return {
    profileRevision,
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
    ready,
    connect,
    disconnect,
    refresh,
    signIn,
    openLogin,
    cancelLogin,
    onEvent,
  }
}

export const useCodexConnectionStore = defineStore('codex-connection', () =>
  createCodexConnection(),
)
