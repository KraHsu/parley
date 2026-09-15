import { beforeEach, afterEach, expect, it, vi } from 'vitest'
import { IDBFactory, IDBKeyRange, IDBObjectStore } from 'fake-indexeddb'
import { readFileSync, writeFileSync } from 'node:fs'
import { initialState, type Word } from './types'
import { dueCards, latestReview } from './review'
import { readLearningBackup } from '../shared/learning-exchange'
import { exportLearning, mergeLearning, previewLearning } from './learning-exchange'
let storage: typeof import('./storage'),
  createLearning: typeof import('./learning').createLearning,
  createPersistence: typeof import('./persistence').createPersistence
beforeEach(async () => {
  vi.resetModules()
  vi.stubGlobal('indexedDB', new IDBFactory())
  vi.stubGlobal('IDBKeyRange', IDBKeyRange)
  storage = await import('./storage')
  ;({ createLearning } = await import('./learning'))
  ;({ createPersistence } = await import('./persistence'))
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})
const nativeWord = (): Word => ({
  id: crypto.randomUUID(),
  text: 'break the ice',
  meaning: '打破冷场',
  language: 'en',
  note: '',
  tags: [],
  source: null,
  createdAt: 1,
  review: null,
})
async function setup(imported = false) {
  const state = initialState()
  state.words = imported
    ? await mergeLearning(
        await previewLearning(
          await readLearningBackup(
            new Blob([readFileSync('fixtures/learning-exchange.json', 'utf8')]),
          ),
          [],
        ),
        [],
        false,
      )
    : [nativeWord()]
  await storage.saveState(state)
  const persistence = createPersistence(state)
  return { state, learning: createLearning(state, persistence, () => true), persistence }
}
it('enrolls both directions, pauses/resumes without resetting, and persists production grades and undo', async () => {
  const { state, learning } = await setup()
  const word = state.words[0]!
  await learning.enroll(word.id)
  await learning.enroll(word.id, 'production')
  expect(dueCards(state.words, Date.now())).toHaveLength(2)
  const original = structuredClone(
    word.learning!.entry.cards.find((c) => c.direction === 'recognition'),
  )
  await learning.review(word, 'hard', 'production', 1)
  expect(word.learning!.entry.cards.find((c) => c.direction === 'recognition')).toEqual(original)
  const history = latestReview(state.words)!.review
  expect(history.afterState.dueAt - history.reviewedAt).toBe(86400000)
  await expect(learning.review(word, 'remembered', 'production', 1)).rejects.toThrow('变化')
  await learning.undoReview(history.id)
  expect(latestReview(state.words)).toBeUndefined()
  expect(dueCards(state.words, Date.now(), 'production')).toHaveLength(1)
  await learning.enroll(word.id, 'production', true)
  expect(dueCards(state.words, Date.now(), 'production')).toHaveLength(0)
  await learning.enroll(word.id, 'production')
  expect(dueCards(state.words, Date.now(), 'production')).toHaveLength(1)
  const restored = await storage.loadState()
  expect(restored!.words).toEqual(state.words)
  const exported = await readLearningBackup(await exportLearning(restored!.words, 'zh-CN'))
  expect(exported.entries[0]!.reviews[0]!.undoneAt).not.toBeNull()
  expect(exported.entries[0]!.cards).toHaveLength(2)
  if (process.env.PARLEY_TEST_PRODUCTION_EXPORT)
    writeFileSync(
      process.env.PARLEY_TEST_PRODUCTION_EXPORT,
      await (await exportLearning(restored!.words, 'zh-CN')).text(),
    )
})
it('leaves memory, due card and history unchanged on an aborted grade so retry grades exactly once', async () => {
  const { state, learning } = await setup()
  const word = state.words[0]!
  await learning.enroll(word.id, 'production')
  const before = JSON.stringify(state)
  const put = IDBObjectStore.prototype.put
  const failure = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (
    this: IDBObjectStore,
    ...args
  ) {
    const request = put.apply(this, args)
    if (this.name === 'words') this.transaction.abort()
    return request
  })
  await expect(learning.review(word, 'remembered', 'production', 1)).rejects.toThrow()
  expect(JSON.stringify(state)).toBe(before)
  failure.mockRestore()
  expect((await storage.loadState())!.words).toEqual(state.words)
  await learning.review(word, 'remembered', 'production', 1)
  expect(word.learning!.entry.reviews).toHaveLength(1)
})
it('requires a meaning and excludes deleted or suspended cards, while preserving desktop version-1 production scheduling', async () => {
  const { state, learning } = await setup(true)
  const word = state.words[0]!
  const card = word.learning!.entry.cards.find((c) => c.direction === 'production')!
  card.scheduleVersion = 1
  card.stage = 5
  card.suspended = false
  card.dueAt = 1
  word.learning!.entry.reviews = word.learning!.entry.reviews.filter((r) => r.cardId !== card.id)
  await storage.saveState(state)
  await learning.review(word, true, 'production', card.revision)
  const graded = word.learning!.entry.cards.find((c) => c.direction === 'production')!
  expect(graded.stage).toBe(5)
  expect(graded.dueAt - graded.lastReviewedAt!).toBe(30 * 86400000)
  await readLearningBackup(await exportLearning(state.words, 'zh-CN'))
  word.meaning = ''
  word.learning!.entry.fields.meaning = ''
  expect(dueCards(state.words, Number.MAX_SAFE_INTEGER).some((i) => i.word.id === word.id)).toBe(
    false,
  )
  await expect(learning.enroll(word.id)).rejects.toThrow('释义')
})
it('refuses undo after the same card has been paused and keeps its new revision', async () => {
  const { state, learning } = await setup()
  const word = state.words[0]!
  await learning.enroll(word.id, 'production')
  await learning.review(word, true, 'production')
  const id = latestReview(state.words)!.review.id
  await learning.enroll(word.id, 'production', true)
  await expect(learning.undoReview(id)).rejects.toThrow('较新的排程')
  expect(word.learning!.entry.cards[0]!.suspended).toBe(true)
})
