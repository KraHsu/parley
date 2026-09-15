import { presets } from '../features/backends/types'
import type { ProfileConfig } from '../features/backends/types'

export const apiPresets = presets.filter(
  (p) => p.config.kind !== 'codex' && p.config.kind !== 'claude_code',
)
export type ApiKind = Exclude<ProfileConfig['kind'], 'codex' | 'claude_code'>
export type Pane = 'main' | 'tutor'
export interface ApiProfile {
  id: string
  name: string
  kind: ApiKind
  provider: ProfileConfig['provider']
  endpoint: string
}
export interface Message {
  id: string
  role: 'user' | 'assistant'
  text: string
  status: 'complete' | 'streaming' | 'interrupted' | 'failed'
  input?: string
  continuation?: unknown
  usage?: Record<string, unknown>
  error?: string
}
export interface Conversation {
  id: string
  pane: Pane
  title: string
  profileId: string
  model: string
  target: string
  native: string
  draft: string
  messages: Message[]
  createdAt: number
}
export interface Word {
  id: string
  text: string
  language: string
  meaning: string
  note: string
  tags: string[]
  source: {
    conversationId: string
    messageId: string
    text: string
    provider: string
    model: string
  } | null
  createdAt: number
  review: { stage: number; dueAt: number; lastReviewedAt: number | null } | null
}
export interface WebState {
  version: 1
  settings: { target: string; native: string }
  profiles: ApiProfile[]
  conversations: Conversation[]
  active: Record<Pane, string>
  words: Word[]
}
export function newConversation(
  pane: Pane,
  profileId = '',
  model = '',
  target = 'en',
  native = 'zh-CN',
): Conversation {
  return {
    id: crypto.randomUUID(),
    pane,
    title: '新的对话',
    profileId,
    model,
    target,
    native,
    draft: '',
    messages: [],
    createdAt: Date.now(),
  }
}
export function initialState(): WebState {
  const main = newConversation('main'),
    tutor = newConversation('tutor')
  return {
    version: 1,
    settings: { target: 'en', native: 'zh-CN' },
    profiles: [],
    conversations: [main, tutor],
    active: { main: main.id, tutor: tutor.id },
    words: [],
  }
}
