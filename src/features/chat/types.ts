export type Pane = 'main' | 'tutor'
export interface VocabularyAnswerTarget {
  id: string
  requestId: string
}
export interface Message {
  id: string
  role: 'user' | 'assistant' | 'note'
  text: string
  status?: 'pending' | 'streaming' | 'complete' | 'interrupted' | 'failed'
  usage?: Record<string, unknown>
  vocabularyTarget?: VocabularyAnswerTarget
}
export interface Conversation {
  id: string
  pane: Pane
  title: string
  model: string
  targetLanguage: string
  nativeLanguage: string
  mode: string
  draft: string
  threadId: string | null
  signature: string
  status: string
  updatedAt: number
  messages: Message[]
  backend?: { profileId: string; profileRevision: number; kind: string } | null
}
export interface Lane {
  id: string
  draft: string
  title: string
  messages: Message[]
  busy: boolean
  error: string
  signature: string
  request: number
  backend: NonNullable<Conversation['backend']>
  apiRequestId?: string
  notice?: string
  vocabularyTarget?: VocabularyAnswerTarget
}
