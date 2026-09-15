import { initialState, type WebState, type Conversation, type Message } from './types'
import {
  validateState,
  validateWord,
  validateProfile,
  validateMessage,
  validateConversation,
} from './validation'
export { validateState } from './validation'
export { exportState, importState, exportBackup, readBackup } from './backup'

const stores = ['workspace', 'profiles', 'conversations', 'messages', 'words'] as const
let database: Promise<IDBDatabase> | undefined
let revision = 0
let writes = Promise.resolve()
export interface Changes {
  meta: boolean
  catalog: boolean
  profiles: Set<string>
  conversations: Set<string>
  words: Set<string>
  messages: Map<string, Set<string>>
}
export function emptyChanges(): Changes {
  return {
    meta: false,
    catalog: false,
    profiles: new Set(),
    conversations: new Set(),
    words: new Set(),
    messages: new Map(),
  }
}
export function mergeChanges(target: Changes, source: Changes) {
  target.meta ||= source.meta
  target.catalog ||= source.catalog
  for (const kind of ['profiles', 'conversations', 'words'] as const)
    for (const id of source[kind]) target[kind].add(id)
  for (const [id, messages] of source.messages) {
    if (!target.messages.has(id)) target.messages.set(id, new Set())
    for (const message of messages) target.messages.get(id)!.add(message)
  }
}
function open(): Promise<IDBDatabase> {
  return (database ??= new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDB.open('parley-web', 2)
    request.onupgradeneeded = () => {
      const db = request.result
      if (!db.objectStoreNames.contains('workspace')) db.createObjectStore('workspace')
      for (const name of ['profiles', 'conversations', 'words'])
        if (!db.objectStoreNames.contains(name)) db.createObjectStore(name, { keyPath: 'id' })
      if (!db.objectStoreNames.contains('messages'))
        db.createObjectStore('messages', { keyPath: ['conversationId', 'id'] }).createIndex(
          'conversationId',
          'conversationId',
        )
    }
    request.onsuccess = () => {
      request.result.onversionchange = () => {
        request.result.close()
        database = undefined
      }
      resolve(request.result)
    }
    request.onerror = () => reject(new Error('无法打开浏览器本地数据，请检查存储权限。'))
    request.onblocked = () => reject(new Error('请关闭其他 Parley 标签页后重试打开数据。'))
  }).catch((error) => {
    database = undefined
    throw error
  }))
}
function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}
type StoredConversation = Omit<Conversation, 'messages'> & { messageIds: string[] }
type StoredMessage = { conversationId: string; id: string; value: Message }
export async function readRawState(): Promise<unknown> {
  const db = await open()
  const tx = db.transaction([...stores])
  // Queue all reads in the same transaction, so they see one consistent revision.
  const [meta, catalog, profiles, conversations, messages, words] = await Promise.all([
    result(tx.objectStore('workspace').get('current')),
    result(tx.objectStore('workspace').get('catalog')),
    result(tx.objectStore('profiles').getAll()),
    result(tx.objectStore('conversations').getAll()) as Promise<StoredConversation[]>,
    result(tx.objectStore('messages').getAll()) as Promise<StoredMessage[]>,
    result(tx.objectStore('words').getAll()),
  ])
  revision = meta?.revision ?? 0
  if (!meta) return initialState()
  if (meta.format !== 2) return meta.state
  const order = <T extends { id: string }>(rows: T[], ids: string[]): T[] => {
    const byId = new Map(rows.map((row) => [row.id, row]))
    return ids.map((id) => byId.get(id)!)
  }
  const byMessage = new Map(
    messages.map((m) => [JSON.stringify([m.conversationId, m.id]), m.value]),
  )
  return {
    version: 1,
    settings: meta.settings,
    active: meta.active,
    profiles: order(profiles, catalog.profiles),
    words: order(words, catalog.words),
    conversations: order(conversations, catalog.conversations).map((c) => ({
      ...c,
      messages: c.messageIds.map((id) => byMessage.get(JSON.stringify([c.id, id]))),
    })),
  }
}
export async function loadState(): Promise<WebState> {
  return validateState(await readRawState(), 'load')
}
const clone = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T
export function saveState(state: WebState, changes?: Changes): Promise<void> {
  // Only dirty records are validated/cloned. Draft keystrokes never visit the word list.
  const full = !changes
  const source = full ? validateState(state, 'save') : state
  const changed = <T extends { id: string }>(rows: T[], ids: Set<string> | undefined) =>
    ids
      ? [...ids].map((id) => ({ id, value: rows.find((row) => row.id === id) }))
      : rows.map((value) => ({ id: value.id, value }))
  const profiles = changed(source.profiles, changes?.profiles).map((r) => ({
    id: r.id,
    value: r.value && validateProfile(r.value),
  }))
  const words = changed(source.words, changes?.words).map((r) => ({
    id: r.id,
    value: r.value && validateWord(r.value),
  }))
  const conversations = changed(source.conversations, changes?.conversations).map((r) => ({
    id: r.id,
    value: r.value && {
      ...validateConversation({ ...r.value, messages: [] }, 'save'),
      messages: undefined,
      messageIds: r.value.messages.map((m) => m.id),
    },
  }))
  const messages: StoredMessage[] = []
  for (const c of source.conversations) {
    const ids = changes?.messages.get(c.id)
    if (!full && !ids) continue
    for (const m of c.messages)
      if (full || ids!.has(m.id))
        messages.push({ conversationId: c.id, id: m.id, value: validateMessage(m, 'save') })
  }
  const snapshot = clone({
    profiles,
    words,
    conversations,
    messages,
    meta: full || changes.meta ? { settings: source.settings, active: source.active } : undefined,
    catalog:
      full || changes.catalog
        ? {
            profiles: source.profiles.map((p) => p.id),
            words: source.words.map((w) => w.id),
            conversations: source.conversations.map((c) => c.id),
          }
        : undefined,
  })
  const task = writes.then(async () => {
    const db = await open()
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction([...stores], 'readwrite')
      const metaStore = tx.objectStore('workspace'),
        request = metaStore.get('current')
      let conflict = false
      request.onsuccess = () => {
        const previous = request.result
        if ((previous?.revision ?? 0) !== revision) {
          conflict = true
          tx.abort()
          return
        }
        if (full) {
          // Keep the original v1 snapshot available until the user explicitly replaces it.
          if (previous?.state) metaStore.put(previous.state, 'legacy-v1')
          for (const name of ['profiles', 'conversations', 'messages', 'words'])
            tx.objectStore(name).clear()
        }
        for (const kind of ['profiles', 'conversations', 'words'] as const)
          for (const row of snapshot[kind]) {
            if (row.value) tx.objectStore(kind).put(row.value)
            else {
              tx.objectStore(kind).delete(row.id)
              if (kind === 'conversations') {
                const cursor = tx
                  .objectStore('messages')
                  .index('conversationId')
                  .openKeyCursor(IDBKeyRange.only(row.id))
                cursor.onsuccess = () => {
                  if (cursor.result) {
                    tx.objectStore('messages').delete(cursor.result.primaryKey)
                    cursor.result.continue()
                  }
                }
              }
            }
          }
        for (const message of snapshot.messages) tx.objectStore('messages').put(message)
        if (snapshot.catalog) metaStore.put(snapshot.catalog, 'catalog')
        metaStore.put(
          {
            format: 2,
            revision: revision + 1,
            settings: snapshot.meta?.settings ?? previous.settings,
            active: snapshot.meta?.active ?? previous.active,
          },
          'current',
        )
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
