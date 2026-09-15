import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest'
import { IDBFactory, IDBKeyRange, IDBObjectStore } from 'fake-indexeddb'
import { initialState, type WebState } from './types'
let storage: typeof import('./storage')
beforeEach(async () => {
  vi.resetModules()
  vi.stubGlobal('indexedDB', new IDBFactory())
  vi.stubGlobal('IDBKeyRange', IDBKeyRange)
  storage = await import('./storage')
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})
function word(id = 'word') {
  return {
    id,
    text: 'hello',
    language: 'en',
    meaning: '你好',
    note: '',
    tags: ['daily'],
    source: null,
    createdAt: 1,
    review: null,
  }
}
async function legacy(state: WebState) {
  await new Promise<void>((resolve, reject) => {
    const request = indexedDB.open('parley-web', 1)
    request.onupgradeneeded = () => request.result.createObjectStore('workspace')
    request.onsuccess = () => {
      const db = request.result,
        tx = db.transaction('workspace', 'readwrite')
      tx.objectStore('workspace').put({ revision: 7, state }, 'current')
      tx.oncomplete = () => {
        db.close()
        resolve()
      }
      tx.onerror = () => reject(tx.error)
    }
  })
}
describe('IndexedDB workspace transactions', () => {
  it('migrates legacy history without losing private continuation, quoted input or word order', async () => {
    const old = initialState()
    old.words = [word('z'), word('a')]
    old.conversations[0]!.messages.push(
      { id: 'user', role: 'user', text: 'visible', input: 'quoted input', status: 'complete' },
      {
        id: 'answer',
        role: 'assistant',
        text: 'hello',
        continuation: [{ encrypted_content: 'opaque' }],
        status: 'complete',
      },
      { id: 'partial', role: 'assistant', text: 'in progress', status: 'streaming' },
    )
    await legacy(old)
    const loaded = await storage.loadState()
    await storage.saveState(loaded)
    expect(await storage.loadState()).toEqual(loaded)
    expect(loaded.words.map((w) => w.id)).toEqual(['z', 'a'])
    expect(loaded.conversations[0]!.messages[0]!.input).toBe('quoted input')
    expect(loaded.conversations[0]!.messages[1]!.continuation).toEqual([
      { encrypted_content: 'opaque' },
    ])
    expect(loaded.conversations[0]!.messages[2]!.status).toBe('interrupted')
  })
  it('writes a draft without serializing or rewriting the word list or message bodies', async () => {
    const state = await storage.loadState()
    state.words = [word()]
    state.conversations[0]!.messages.push({
      id: 'a',
      role: 'assistant',
      text: 'unchanged',
      status: 'complete',
    })
    await storage.saveState(state)
    const put = vi.spyOn(IDBObjectStore.prototype, 'put')
    const visit = vi.fn(() => {
      throw new Error('draft write visited words')
    })
    Object.defineProperty(state.words[0]!, 'meaning', { get: visit })
    state.conversations[0]!.draft = 'new draft'
    const changes = storage.emptyChanges()
    changes.conversations.add(state.active.main)
    await storage.saveState(state, changes)
    expect(visit).not.toHaveBeenCalled()
    expect(put.mock.contexts.map((store) => (store as IDBObjectStore).name).sort()).toEqual([
      'conversations',
      'workspace',
    ])
    const loaded = await storage.loadState()
    expect(loaded.conversations[0]!.draft).toBe('new draft')
    expect(loaded.conversations[0]!.messages[0]!.text).toBe('unchanged')
    expect(loaded.words[0]!.meaning).toBe('你好')
  })
  it('rolls back the entire replacement on an aborted write, including cleared stores', async () => {
    const original = await storage.loadState()
    original.words.push(word('original'))
    await storage.saveState(original)
    const backup = initialState()
    backup.words.push(word('replacement'))
    const put = IDBObjectStore.prototype.put
    const failure = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (
      this: IDBObjectStore,
      ...args: Parameters<typeof put>
    ) {
      const request = put.apply(this, args)
      if (this.name === 'words') this.transaction.abort()
      return request
    })
    await expect(storage.saveState(backup)).rejects.toThrow('本地保存失败')
    failure.mockRestore()
    expect(await storage.loadState()).toEqual(original)
  })
  it('rejects a stale tab revision without losing the first tab changes', async () => {
    const first = await storage.loadState()
    await storage.saveState(first)
    vi.resetModules()
    const secondStorage = await import('./storage')
    const second = await secondStorage.loadState()
    first.words.push(word('first'))
    await storage.saveState(first)
    second.words.push(word('second'))
    await expect(secondStorage.saveState(second)).rejects.toThrow('另一个标签页')
    expect((await storage.loadState()).words.map((w) => w.id)).toEqual(['first'])
  })
  it('rejects invalid writes before changing any durable data and exposes the original legacy data for recovery', async () => {
    const original = initialState()
    original.words.push({ ...word(), tags: ['x'.repeat(101)] })
    await legacy(original)
    await expect(storage.loadState()).rejects.toThrow('每个标签')
    expect(await storage.readRawState()).toEqual(original)
    expect(() => storage.saveState(original)).toThrow('每个标签')
    original.words[0]!.tags = ['fixed']
    await storage.saveState(original)
    expect(await storage.loadState()).toEqual(original)
  })
})
