import { apiPresets, initialState, type WebState } from './types'

const STORE = 'workspace'
let database: Promise<IDBDatabase> | undefined
let revision = 0
let writes = Promise.resolve()
function open(): Promise<IDBDatabase> {
  return (database ??= new Promise((resolve, reject) => {
    const request = indexedDB.open('parley-web', 1)
    request.onupgradeneeded = () => request.result.createObjectStore(STORE)
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(new Error('无法打开浏览器本地数据，请检查浏览器存储权限。'))
  }))
}
function assert(condition: unknown): asserts condition {
  if (!condition)
    throw new Error('备份格式无效；请选择 Parley Web v1 备份。桌面备份请在桌面端导入。')
}
// Reconstruct only known fields. API credentials and unexpected imported keys are never retained.
export function validateState(input: unknown): WebState {
  assert(input && typeof input === 'object')
  const v = input as WebState
  const text = (s: unknown, max = 2_000_000): string => {
    assert(typeof s === 'string' && s.length <= max)
    return s
  }
  const time = (n: unknown): number => {
    assert(typeof n === 'number' && Number.isFinite(n) && n >= 0)
    return n
  }
  assert(v.version === 1 && v.settings && v.active)
  assert(Array.isArray(v.profiles) && v.profiles.length <= 100)
  assert(Array.isArray(v.conversations) && v.conversations.length <= 10000)
  assert(Array.isArray(v.words) && v.words.length <= 50000)
  const profiles = v.profiles.map((p) => {
    assert(
      apiPresets.some(
        (preset) => preset.config.kind === p.kind && preset.config.provider === p.provider,
      ),
    )
    return {
      id: text(p.id, 100),
      name: text(p.name, 100),
      kind: p.kind,
      provider: p.provider,
      endpoint: text(p.endpoint, 2048),
    }
  })
  const conversations = v.conversations.map((c) => {
    assert(c.pane === 'main' || c.pane === 'tutor')
    assert(!c.profileId || profiles.some((p) => p.id === c.profileId))
    assert(Array.isArray(c.messages) && c.messages.length <= 10000)
    return {
      id: text(c.id, 100),
      pane: c.pane,
      title: text(c.title, 200),
      profileId: text(c.profileId, 100),
      model: text(c.model, 200),
      target: text(c.target, 100),
      native: text(c.native, 100),
      draft: text(c.draft),
      createdAt: time(c.createdAt),
      messages: c.messages.map((m) => {
        assert(m.role === 'user' || m.role === 'assistant')
        assert(['complete', 'failed', 'streaming', 'interrupted'].includes(m.status))
        // Backups deliberately omit opaque continuation and request input. Visible history is portable.
        return {
          id: text(m.id, 200),
          role: m.role,
          text: text(m.text),
          status: m.status === 'streaming' ? ('interrupted' as const) : m.status,
        }
      }),
    }
  })
  assert(conversations.some((c) => c.id === v.active.main && c.pane === 'main'))
  assert(conversations.some((c) => c.id === v.active.tutor && c.pane === 'tutor'))
  const words = v.words.map((w) => {
    assert(Array.isArray(w.tags) && w.tags.length <= 30)
    const source = w.source
      ? {
          conversationId: text(w.source.conversationId, 100),
          messageId: text(w.source.messageId, 200),
          text: text(w.source.text),
          provider: text(w.source.provider, 100),
          model: text(w.source.model, 200),
        }
      : null
    if (w.review)
      assert(Number.isInteger(w.review.stage) && w.review.stage >= 0 && w.review.stage <= 6)
    return {
      id: text(w.id, 100),
      text: text(w.text, 10000),
      language: text(w.language, 100),
      meaning: text(w.meaning, 10000),
      note: text(w.note, 10000),
      tags: w.tags.map((t) => text(t, 100)),
      source,
      createdAt: time(w.createdAt),
      review: w.review
        ? {
            stage: w.review.stage,
            dueAt: time(w.review.dueAt),
            lastReviewedAt: w.review.lastReviewedAt === null ? null : time(w.review.lastReviewedAt),
          }
        : null,
    }
  })
  for (const rows of [profiles, conversations, words])
    assert(new Set(rows.map((r) => r.id)).size === rows.length)
  for (const c of conversations)
    assert(new Set(c.messages.map((m) => m.id)).size === c.messages.length)
  return {
    version: 1,
    settings: { target: text(v.settings.target, 100), native: text(v.settings.native, 100) },
    profiles,
    conversations,
    active: { main: v.active.main, tutor: v.active.tutor },
    words,
  }
}
export async function loadState(): Promise<WebState> {
  const db = await open()
  const stored = await new Promise<{ revision: number; state: WebState } | undefined>(
    (resolve, reject) => {
      const request = db.transaction(STORE).objectStore(STORE).get('current')
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    },
  )
  revision = stored?.revision ?? 0
  if (!stored) return initialState()
  const state = validateState(stored.state)
  // Locally generated protocol continuation is needed for native API multi-turn history.
  for (const conversation of state.conversations) {
    const original = stored.state.conversations.find((c) => c.id === conversation.id)!
    conversation.messages.forEach((message, i) => {
      const previous = original.messages[i]!
      if (message.status === 'complete') {
        message.continuation = previous.continuation
        message.input = previous.input
        message.usage = previous.usage
      }
    })
  }
  return state
}
export function saveState(state: WebState): Promise<void> {
  const snapshot = JSON.parse(JSON.stringify(state)) as WebState
  const task = writes.then(async () => {
    const db = await open()
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE, 'readwrite')
      const store = tx.objectStore(STORE),
        request = store.get('current')
      let conflict = false
      request.onsuccess = () => {
        if ((request.result?.revision ?? 0) !== revision) {
          conflict = true
          tx.abort()
          return
        }
        store.put({ revision: revision + 1, state: snapshot }, 'current')
      }
      tx.oncomplete = () => {
        revision++
        resolve()
      }
      tx.onabort = tx.onerror = () =>
        reject(
          new Error(
            conflict
              ? '数据已被另一个标签页更新，请刷新本页后继续。'
              : '本地保存失败。请导出备份并检查浏览器存储空间。',
          ),
        )
    })
  })
  writes = task.catch(() => {})
  return task
}
export function exportState(state: WebState): string {
  return JSON.stringify(
    { app: 'parley-web', exportedAt: new Date().toISOString(), state: validateState(state) },
    null,
    2,
  )
}
export function importState(raw: string): WebState {
  if (new TextEncoder().encode(raw).length > 32 * 1024 * 1024)
    throw new Error('备份不能超过 32 MiB。')
  const input = JSON.parse(raw)
  assert(input.app === 'parley-web')
  return validateState(input.state)
}
