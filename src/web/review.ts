import type { ReviewCard } from '../features/vocabulary/types'
import type { LearningReview } from '../shared/learning-exchange'
import type { Word } from './types'
export type Direction = ReviewCard['direction']
export interface ReviewItem {
  word: Word
  card: ReviewCard
}
export function wordCards(word: Word): ReviewCard[] {
  if (word.learning) return word.learning.entry.cards
  return word.review
    ? [
        {
          id: word.id,
          entryId: word.id,
          direction: 'recognition',
          ...word.review,
          suspended: false,
          scheduleVersion: 2,
          revision: 1,
        },
      ]
    : []
}
export function dueCards(words: Word[], now: number, direction?: Direction): ReviewItem[] {
  return words
    .filter((w) => w.learning?.entry.deletedAt == null && w.meaning.trim())
    .flatMap((word) =>
      wordCards(word)
        .filter(
          (card) =>
            !card.suspended && card.dueAt <= now && (!direction || card.direction === direction),
        )
        .map((card) => ({ word, card })),
    )
    .sort((a, b) => a.card.dueAt - b.card.dueAt || a.card.id.localeCompare(b.card.id))
}
export function latestReview(words: Word[]): { word: Word; review: LearningReview } | undefined {
  return words
    .flatMap(
      (word) =>
        word.learning?.entry.reviews
          .filter((r) => r.undoneAt === null)
          .map((review) => ({ word, review })) ?? [],
    )
    .sort(
      (a, b) => b.review.reviewedAt - a.review.reviewedAt || b.review.id.localeCompare(a.review.id),
    )[0]
}
