export interface EntryFields {
  language: string
  languageLabel: string
  kind: 'word' | 'phrase' | 'sentence'
  text: string
  meaning: string
  meaningLanguage: string
  note: string
}
export interface VocabularySource {
  sourceKind: 'main' | 'tutor' | 'terminal' | 'manual' | 'import'
  conversationId: string | null
  messageId: string | null
  threadId: string | null
  turnId: string | null
  itemId: string | null
  role: 'user' | 'assistant' | 'manual'
  selectedText: string
  snapshot: string
  start: number
  end: number
  locatorVersion: number
  truncated: boolean
}
export interface Occurrence extends VocabularySource {
  id: string
  snapshotHash: string
}
export interface VocabularyEntry extends EntryFields {
  id: string
  revision: number
  createdAt: number
  updatedAt: number
  deletedAt: number | null
  occurrences: Occurrence[]
  tags: string[]
  cards: ReviewCard[]
}
export interface EditDraft {
  tagText: string
  id: string
  entryId: string | null
  baseRevision: number | null
  fields: EntryFields
  occurrence: VocabularySource | null
  allowDuplicate: boolean
  requestId: string
}
export interface SaveResult {
  entry: VocabularyEntry
  duplicate: boolean
}
export interface ListQuery {
  search: string
  language: string
  kind: string
  hasMeaning: boolean | null
  trash: boolean
  sort: string
  offset: number
  tag: string
  reviewStatus: string
}
export interface EntryPage {
  entries: VocabularyEntry[]
  total: number
  languages: string[]
  tags: string[]
  activeCount: number
}

export interface ReviewCard {
  id: string
  entryId: string
  direction: 'recognition' | 'production'
  stage: number
  dueAt: number
  lastReviewedAt: number | null
  suspended: boolean
  scheduleVersion: number
  revision: number
}
export interface ReviewItem {
  card: ReviewCard
  entry: VocabularyEntry
}
export interface ReviewQueue {
  items: ReviewItem[]
  dueCount: number
  newCount: number
  nextDueAt: number | null
  completedToday: number
  lastReviewId: string | null
}
