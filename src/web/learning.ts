import { syncLearningFields } from './learning-exchange'
import { scheduleCard } from '../shared/learning-exchange'
import type { WebState, Word } from './types'
import { validateWord, limits } from './validation'
import { sameLearningText } from '../shared/learning-fields'
import type { createPersistence } from './persistence'

export function createLearning(
  state: WebState,
  persistence: ReturnType<typeof createPersistence>,
  canEdit: () => boolean,
) {
  function editable() {
    if (!canEdit()) throw new Error('当前无法修改，请先处理保存错误或只读状态。')
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
  function enroll(id: string) {
    editable()
    const word = state.words.find((w) => w.id === id)
    if (!word || word.review) return
    const entry = word.learning?.entry
    if (entry) {
      let card = entry.cards.find((c) => c.direction === 'recognition')
      if (card) {
        card.suspended = false
        card.revision++
      } else {
        card = {
          id: crypto.randomUUID(),
          entryId: word.id,
          direction: 'recognition',
          stage: 0,
          dueAt: Date.now(),
          lastReviewedAt: null,
          suspended: false,
          scheduleVersion: 2,
          revision: 1,
        }
        entry.cards.push(card)
      }
      word.review = { stage: card.stage, dueAt: card.dueAt, lastReviewedAt: card.lastReviewedAt }
      entry.updatedAt = Date.now()
    } else word.review = { stage: 0, dueAt: Date.now(), lastReviewedAt: null }
    persistence.mark('words', id)
  }
  function review(word: Word, remembered: boolean) {
    editable()
    const saved = state.words.find((w) => w.id === word.id)
    if (!saved?.review) return
    const entry = saved.learning?.entry
    const card = entry?.cards.find((c) => c.direction === 'recognition' && !c.suspended)
    if (entry && card) {
      const at = Date.now(),
        rating = remembered ? 'remembered' : 'forgot'
      const before = { ...card },
        after = scheduleCard(card, rating, at)
      entry.reviews.push({
        id: crypto.randomUUID(),
        cardId: card.id,
        rating,
        reviewedAt: at,
        beforeState: before,
        afterState: after,
        undoneAt: null,
      })
      Object.assign(card, after)
      saved.review = { stage: card.stage, dueAt: card.dueAt, lastReviewedAt: card.lastReviewedAt }
      entry.updatedAt = at
      persistence.mark('words', saved.id)
      return
    }
    const stage = remembered ? Math.min(6, saved.review.stage + 1) : 0
    const days = [0, 1, 3, 7, 14, 30, 60][stage]!
    saved.review = {
      stage,
      dueAt: Date.now() + (days ? days * 86400000 : 600000),
      lastReviewedAt: Date.now(),
    }
    persistence.mark('words', saved.id)
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
  return { saveWord, removeWord, enroll, review, restoreWord }
}
