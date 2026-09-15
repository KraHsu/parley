import {
  apiPresets,
  type WebState,
  type Word,
  type ApiProfile,
  type Conversation,
  type Message,
} from './types'
import { boundedText, normalizeTags } from '../shared/learning-fields'

export type ValidationMode = 'backup' | 'load' | 'save'
export const limits = { profiles: 100, conversations: 10000, words: 50000, messages: 10000 }
function assert(value: unknown, label = '备份格式'): asserts value {
  if (!value) throw new Error(`${label}无效，请检查数据。`)
}
function object(input: unknown): Record<string, unknown> {
  assert(input && typeof input === 'object' && !Array.isArray(input))
  return input as Record<string, unknown>
}
const text = (s: unknown, max = 2_000_000, label = '文本') => boundedText(s, max, label)
function time(input: unknown): number {
  assert(typeof input === 'number' && Number.isFinite(input) && input >= 0, '时间')
  return input
}
export function validateProfile(input: unknown): ApiProfile {
  const p = object(input)
  const preset = apiPresets.find(
    (v) => v.config.kind === p.kind && v.config.provider === p.provider,
  )
  assert(preset, 'API 服务类型')
  return {
    id: text(p.id, 100),
    name: text(p.name, 100, '配置名称'),
    kind: preset.config.kind as ApiProfile['kind'],
    provider: preset.config.provider,
    endpoint: text(p.endpoint, 2048, '服务地址'),
  }
}
export function validateMessage(input: unknown, mode: ValidationMode): Message {
  const m = object(input)
  assert(m.role === 'user' || m.role === 'assistant', '消息角色')
  assert(['complete', 'failed', 'streaming', 'interrupted'].includes(String(m.status)), '消息状态')
  const status =
    mode !== 'save' && m.status === 'streaming' ? 'interrupted' : (m.status as Message['status'])
  const result: Message = { id: text(m.id, 200), role: m.role, text: text(m.text), status }
  if (mode !== 'backup') {
    if (m.error !== undefined) result.error = text(m.error)
    if (status === 'complete' || mode === 'save') {
      if (m.input !== undefined) result.input = text(m.input)
      if (m.continuation !== undefined) result.continuation = m.continuation
      if (m.usage !== undefined) result.usage = object(m.usage)
    }
  }
  return result
}
export function validateConversation(input: unknown, mode: ValidationMode): Conversation {
  const c = object(input)
  assert(c.pane === 'main' || c.pane === 'tutor', '会话面板')
  assert(Array.isArray(c.messages) && c.messages.length <= limits.messages, '消息数量')
  const messages = c.messages.map((m) => validateMessage(m, mode))
  unique(messages)
  return {
    id: text(c.id, 100),
    pane: c.pane,
    title: text(c.title, 200),
    profileId: text(c.profileId, 100),
    model: text(c.model, 200, '模型 ID'),
    target: text(c.target, 100, '目标语言'),
    native: text(c.native, 100, '母语'),
    draft: text(c.draft),
    createdAt: time(c.createdAt),
    messages,
  }
}
export function validateWord(input: unknown): Word {
  const w = object(input)
  const s = w.source == null ? null : object(w.source)
  const r = w.review == null ? null : object(w.review)
  if (r)
    assert(Number.isInteger(r.stage) && Number(r.stage) >= 0 && Number(r.stage) <= 6, '复习阶段')
  return {
    id: text(w.id, 100),
    text: text(w.text, 10000, '词句'),
    language: text(w.language, 100, '语言代码'),
    meaning: text(w.meaning, 10000, '释义'),
    note: text(w.note, 10000, '注释'),
    tags: normalizeTags(w.tags),
    createdAt: time(w.createdAt),
    source: s
      ? {
          conversationId: text(s.conversationId, 100),
          messageId: text(s.messageId, 200),
          text: text(s.text),
          provider: text(s.provider, 100),
          model: text(s.model, 200),
        }
      : null,
    review: r
      ? {
          stage: Number(r.stage),
          dueAt: time(r.dueAt),
          lastReviewedAt: r.lastReviewedAt === null ? null : time(r.lastReviewedAt),
        }
      : null,
  }
}
export function unique(rows: { id: string }[]) {
  assert(new Set(rows.map((r) => r.id)).size === rows.length, '重复记录 ID')
}
export function validateState(input: unknown, mode: ValidationMode = 'backup'): WebState {
  const v = object(input),
    settings = object(v.settings),
    active = object(v.active)
  assert(v.version === 1, 'Web 数据版本')
  for (const [key, max] of Object.entries(limits).filter(([k]) => k !== 'messages'))
    assert(Array.isArray(v[key]) && (v[key] as unknown[]).length <= max, `${key} 数量`)
  const profiles = (v.profiles as unknown[]).map(validateProfile)
  const conversations = (v.conversations as unknown[]).map((c) => validateConversation(c, mode))
  const words = (v.words as unknown[]).map(validateWord)
  for (const rows of [profiles, conversations, words]) unique(rows)
  for (const c of conversations)
    assert(!c.profileId || profiles.some((p) => p.id === c.profileId), '会话的服务引用')
  for (const pane of ['main', 'tutor'] as const)
    assert(
      conversations.some((c) => c.id === active[pane] && c.pane === pane),
      '当前会话',
    )
  return {
    version: 1,
    settings: {
      target: text(settings.target, 100, '目标语言'),
      native: text(settings.native, 100, '母语'),
    },
    profiles,
    conversations,
    words,
    active: { main: active.main as string, tutor: active.tutor as string },
  }
}
