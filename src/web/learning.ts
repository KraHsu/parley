import { ref } from 'vue'
import { emptyChanges, saveState } from './storage'
import { latestReview, type Direction } from './review'
import { syncLearningFields, wordEntry } from './learning-exchange'
import {
  canonical,
  digest,
  validateLearningEntry,
  type LearningEntry,
  type LearningReview,
  scheduleCard,
} from '../shared/learning-exchange'
import type { WebState, Word } from './types'
import { validateWord, limits } from './validation'
import { sameLearningText } from '../shared/learning-fields'
import type { createPersistence } from './persistence'

export function createLearning(
  state: WebState,
  persistence: ReturnType<typeof createPersistence>,
  canEdit: () => boolean,
) {
  const learningBusy = ref(false)
  function editable() {
    if (!canEdit() || learningBusy.value)
      throw new Error('当前无法修改，请先处理保存错误或只读状态。')
  }
  async function saveWord(input: Word) {
    editable()
    const word = validateWord(input)
    word.text = word.text.trim()
    if (!word.text) throw new Error('请填写词句。')
    const previous = state.words.find((w) => w.id === word.id)
    if (
      (!previous ||
        previous.language !== word.language ||
        !sameLearningText(previous.text, word.text)) &&
      state.words.some(
        (w) =>
          w.id !== word.id && w.language === word.language && sameLearningText(w.text, word.text),
      )
    )
      throw new Error('已有相同词句，请在词句列表中编辑原记录。')
    word.updatedAt = Date.now()
    syncLearningFields(word)
    const existing = state.words.find((w) => w.id === word.id)
    if (existing) Object.assign(existing, word)
    else {
      if (state.words.length >= limits.words)
        throw new Error('词句数量已达到当前工作区上限，请先导出并整理词句。')
      state.words.unshift(word)
      persistence.mark('catalog')
    }
    persistence.mark('words', word.id)
    if (!(await persistence.persist())) throw new Error(persistence.storageError.value)
  }
  async function removeWord(id: string) {
    editable()
    state.words = state.words.filter((w) => w.id !== id)
    persistence.mark('catalog')
    persistence.mark('words', id)
    if (!(await persistence.persist())) throw new Error(persistence.storageError.value)
  }
  // Commit a complete card/history change before replacing the visible word.
  // A failed write leaves the displayed question and schedule available to retry.
  async function changeLearning(id: string, change: (entry: LearningEntry) => void) {
    editable()
    learningBusy.value = true
    try {
      if (!(await persistence.persist())) throw new Error(persistence.storageError.value)
      const original = state.words.find((w) => w.id === id)
      if (!original) throw new Error('词句已不存在。')
      const word = JSON.parse(JSON.stringify(original)) as Word
      const entry = await wordEntry(word, state.settings.native)
      change(entry)
      entry.updatedAt = Date.now()
      const checked = validateLearningEntry(entry)
      word.id = checked.id
      word.learning = word.learning
        ? { ...word.learning, entry: checked }
        : {
            datasetId: 'a74ef028-c29c-487e-9456-29a8e716b074',
            recordId: checked.id,
            fingerprint: await digest(canonical(checked)),
            entry: checked,
          }
      const recognition = checked.cards.find((c) => c.direction === 'recognition' && !c.suspended)
      word.review = recognition
        ? {
            stage: recognition.stage,
            dueAt: recognition.dueAt,
            lastReviewedAt: recognition.lastReviewedAt,
          }
        : null
      word.updatedAt = entry.updatedAt
      const changes = emptyChanges()
      changes.words.add(id)
      changes.words.add(word.id)
      changes.catalog = id !== word.id
      await saveState(
        { ...state, words: state.words.map((w) => (w.id === id ? word : w)) },
        changes,
      )
      Object.assign(original, word)
    } finally {
      learningBusy.value = false
    }
  }
  async function enroll(id: string, direction: Direction = 'recognition', suspended = false) {
    await changeLearning(id, (entry) => {
      if (entry.deletedAt !== null) throw new Error('请先恢复词句。')
      if (!suspended && !entry.fields.meaning.trim()) throw new Error('请先补充释义，再加入复习。')
      let card = entry.cards.find((c) => c.direction === direction)
      if (card) {
        if (card.suspended !== suspended) {
          card.suspended = suspended
          card.revision++
        }
      } else if (!suspended) {
        card = {
          id: crypto.randomUUID(),
          entryId: entry.id,
          direction,
          stage: 0,
          dueAt: Date.now(),
          lastReviewedAt: null,
          suspended: false,
          scheduleVersion: 2,
          revision: 1,
        }
        entry.cards.push(card)
      }
    })
  }
  async function review(
    word: Word,
    rating: boolean | LearningReview['rating'],
    direction: Direction = 'recognition',
    expectedRevision?: number,
  ) {
    const at = Math.max(Date.now(), (latestReview(state.words)?.review.reviewedAt ?? 0) + 1)
    await changeLearning(word.id, (entry) => {
      const card = entry.cards.find((c) => c.direction === direction && !c.suspended)
      if (!card || entry.deletedAt !== null || !entry.fields.meaning.trim())
        throw new Error('此卡片当前不能复习。')
      if (expectedRevision !== undefined && card.revision !== expectedRevision)
        throw new Error('卡片已变化，请重新开始复习。')
      if (card.dueAt > at) throw new Error('此卡片尚未到复习时间。')
      const grade = typeof rating === 'boolean' ? (rating ? 'remembered' : 'forgot') : rating
      const beforeState = { ...card },
        afterState = scheduleCard(card, grade, at)
      entry.reviews.push({
        id: crypto.randomUUID(),
        cardId: card.id,
        rating: grade,
        reviewedAt: at,
        beforeState,
        afterState,
        undoneAt: null,
      })
      Object.assign(card, afterState)
    })
  }
  async function undoReview(id: string) {
    const last = latestReview(state.words)
    if (!last || last.review.id !== id) throw new Error('只能撤销最近一次评分。')
    await changeLearning(last.word.id, (entry) => {
      const record = entry.reviews.find((r) => r.id === id)!
      const card = entry.cards.find((c) => c.id === record.cardId)
      if (!card || canonical(card) !== canonical(record.afterState))
        throw new Error('评分后卡片已有变化，不能覆盖较新的排程。')
      Object.assign(card, record.beforeState, { revision: card.revision + 1 })
      record.undoneAt = Math.max(Date.now(), record.reviewedAt)
    })
  }
  async function restoreWord(id: string) {
    editable()
    const word = state.words.find((w) => w.id === id)
    if (!word?.learning) return
    word.learning.entry.deletedAt = null
    word.learning.entry.updatedAt = Date.now()
    persistence.mark('words', word.id)
    if (!(await persistence.persist())) throw new Error(persistence.storageError.value)
  }
  return { saveWord, removeWord, enroll, review, undoReview, restoreWord, learningBusy }
}
