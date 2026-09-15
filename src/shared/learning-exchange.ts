import type { EntryFields, Occurrence, ReviewCard } from '../features/vocabulary/types'
import { boundedText, normalizeTags } from './learning-fields'

export interface LearningReview {
  id: string
  cardId: string
  rating: 'forgot' | 'hard' | 'remembered'
  reviewedAt: number
  beforeState: ReviewCard
  afterState: ReviewCard
  undoneAt: number | null
}
export interface LearningEntry {
  id: string
  fields: EntryFields
  createdAt: number
  updatedAt: number
  deletedAt: number | null
  occurrences: Occurrence[]
  tags: string[]
  cards: ReviewCard[]
  reviews: LearningReview[]
}
export interface LearningBackup {
  format: 'parley-vocabulary'
  version: number
  datasetId: string
  exportedAt: number
  entries: LearningEntry[]
}
export interface LearningOrigin {
  datasetId: string
  fingerprint: string
  recordId: string
  entry: LearningEntry
}
export const maxLearningBytes = 512 * 1024 * 1024
function requireValue(value: unknown, label: string): asserts value {
  if (!value) throw new Error(`词句备份中的${label}无效。`)
}
function object(value: unknown): Record<string, unknown> {
  requireValue(value && typeof value === 'object' && !Array.isArray(value), '记录')
  return value as Record<string, unknown>
}
function list(value: unknown, max: number, label: string): unknown[] {
  requireValue(Array.isArray(value) && value.length <= max, label)
  return value
}
export function learningId(value: unknown): string {
  requireValue(
    typeof value === 'string' &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value),
    '标识',
  )
  return value
}
function time(value: unknown): number {
  requireValue(typeof value === 'number' && Number.isSafeInteger(value) && value >= 0, '时间')
  return value
}
const nullableTime = (v: unknown) => (v === null ? null : time(v))
function utf8Order(a: string, b: string): number {
  const x = Array.from(a, (c) => c.codePointAt(0)!),
    y = Array.from(b, (c) => c.codePointAt(0)!)
  for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i]! - y[i]!
  return x.length - y.length
}
const text = (v: unknown, n: number, label: string) => boundedText(v, n, label)
function bool(v: unknown): boolean {
  requireValue(typeof v === 'boolean', '布尔值')
  return v
}
function language(value: unknown): string {
  const code = text(value, 100, '语言代码')
  // Keep BCP-47 private-use/grandfathered tags supported by the desktop parser.
  requireValue(/^[A-Za-z]{1,8}(?:-[A-Za-z0-9]{1,8})*$/.test(code), '语言代码')
  return code
}
export function scheduleCard(
  card: ReviewCard,
  rating: LearningReview['rating'],
  at: number,
): ReviewCard {
  const max = card.scheduleVersion === 1 ? 5 : 6
  const stage =
    rating === 'remembered' ? Math.min(max, card.stage + 1) : rating === 'forgot' ? 0 : card.stage
  const delay =
    rating === 'forgot'
      ? 600000
      : rating === 'hard'
        ? 86400000
        : [1, 3, 7, 14, 30, 60][Math.min(max - 1, card.stage)]! * 86400000
  return { ...card, stage, dueAt: at + delay, lastReviewedAt: at, revision: card.revision + 1 }
}
function card(value: unknown, entryId: string): ReviewCard {
  const v = object(value)
  requireValue(
    v.entryId === entryId && ['recognition', 'production'].includes(String(v.direction)),
    '卡片归属',
  )
  requireValue(
    typeof v.scheduleVersion === 'number' && [1, 2].includes(v.scheduleVersion),
    '排程版本',
  )
  requireValue(
    Number.isInteger(v.stage) &&
      Number(v.stage) >= 0 &&
      Number(v.stage) <= (v.scheduleVersion === 1 ? 5 : 6),
    '复习阶段',
  )
  requireValue(Number.isSafeInteger(v.revision) && Number(v.revision) >= 1, '复习版本')
  return {
    id: learningId(v.id),
    entryId,
    direction: v.direction as ReviewCard['direction'],
    stage: Number(v.stage),
    dueAt: time(v.dueAt),
    lastReviewedAt: nullableTime(v.lastReviewedAt),
    suspended: bool(v.suspended),
    scheduleVersion: Number(v.scheduleVersion),
    revision: Number(v.revision),
  }
}
function occurrence(value: unknown): Occurrence {
  const v = object(value)
  requireValue(
    ['main', 'tutor', 'terminal', 'manual', 'import'].includes(String(v.sourceKind)),
    '来源类型',
  )
  requireValue(
    ['user', 'assistant', 'manual'].includes(String(v.role)) && v.locatorVersion === 1,
    '选区类型',
  )
  const snapshot = text(v.snapshot, 2_000_000, '来源原文'),
    selectedText = text(v.selectedText, 2_000_000, '选中文字')
  const chars = Array.from(snapshot)
  requireValue(
    Number.isInteger(v.start) &&
      Number.isInteger(v.end) &&
      Number(v.start) >= 0 &&
      Number(v.end) > Number(v.start) &&
      Number(v.end) <= chars.length &&
      chars.slice(Number(v.start), Number(v.end)).join('') === selectedText,
    '原文选区',
  )
  // Check only the two boundaries; materializing every grapheme of a large
  // migrated passage makes import validation unnecessarily slow and memory-heavy.
  const segments = new Intl.Segmenter().segment(snapshot)
  for (const scalar of [Number(v.start), Number(v.end)]) {
    if (scalar === 0 || scalar === chars.length) continue
    const utf16 = chars.slice(0, scalar).join('').length
    requireValue(segments.containing(utf16)?.index === utf16, '字符边界')
  }
  const optionalId = (v: unknown) => (v === null ? null : text(v, 200, '来源标识'))
  const result: Occurrence = {
    id: learningId(v.id),
    sourceKind: v.sourceKind as Occurrence['sourceKind'],
    conversationId: optionalId(v.conversationId),
    messageId: optionalId(v.messageId),
    threadId: optionalId(v.threadId),
    turnId: optionalId(v.turnId),
    itemId: optionalId(v.itemId),
    role: v.role as Occurrence['role'],
    selectedText,
    snapshot,
    start: Number(v.start),
    end: Number(v.end),
    locatorVersion: 1,
    truncated: bool(v.truncated),
    snapshotHash: text(v.snapshotHash, 64, '原文校验值'),
  }
  requireValue((result.conversationId === null) === (result.messageId === null), '来源会话')
  if (v.backend !== undefined) {
    const b = object(v.backend)
    requireValue(
      ['openai', 'anthropic', 'google', 'deepseek', 'qwen', 'kimi', 'zai', 'custom'].includes(
        String(b.provider),
      ),
      '来源厂商',
    )
    requireValue(
      b.kind === 'openai_compatible' ||
        (['codex', 'openai_responses'].includes(String(b.kind)) && b.provider === 'openai') ||
        (['claude_code', 'anthropic_messages'].includes(String(b.kind)) &&
          b.provider === 'anthropic') ||
        (b.kind === 'gemini_interactions' && b.provider === 'google'),
      '来源协议',
    )
    requireValue(result.sourceKind !== 'manual', '手动来源')
    result.backend = {
      kind: b.kind as NonNullable<Occurrence['backend']>['kind'],
      provider: b.provider as NonNullable<Occurrence['backend']>['provider'],
      model: b.model === null ? null : text(b.model, 200, '来源模型'),
    }
  }
  if (result.sourceKind === 'terminal')
    requireValue(
      result.threadId &&
        result.itemId &&
        (!result.backend || ['codex', 'claude_code'].includes(result.backend.kind)),
      '终端来源',
    )
  return result
}
export function validateLearningEntry(value: unknown): LearningEntry {
  const v = object(value),
    f = object(v.fields),
    id = learningId(v.id)
  requireValue(['word', 'phrase', 'sentence'].includes(String(f.kind)), '词句类型')
  const fields: EntryFields = {
    language: language(f.language),
    languageLabel: text(f.languageLabel ?? '', 100, '语言名称'),
    kind: f.kind as EntryFields['kind'],
    text: text(f.text, 10000, '词句'),
    meaning: text(f.meaning ?? '', 10000, '释义'),
    meaningLanguage: language(f.meaningLanguage),
    note: text(f.note ?? '', 16000, '注释'),
  }
  requireValue(fields.text.trim(), '词句内容')
  const cards = list(v.cards, 2, '卡片数量').map((v) => card(v, id))
  requireValue(new Set(cards.map((c) => c.direction)).size === cards.length, '卡片方向')
  const reviews = list(v.reviews, 100000, '复习次数').map((value) => {
    const r = object(value)
    requireValue(
      ['forgot', 'hard', 'remembered'].includes(String(r.rating)) &&
        cards.some((c) => c.id === r.cardId),
      '复习记录',
    )
    const beforeState = card(r.beforeState, id),
      afterState = card(r.afterState, id),
      reviewedAt = time(r.reviewedAt)
    requireValue(beforeState.id === r.cardId && afterState.id === r.cardId, '复习卡片')
    requireValue(
      canonical(scheduleCard(beforeState, r.rating as LearningReview['rating'], reviewedAt)) ===
        canonical(afterState),
      '复习排程',
    )
    return {
      id: learningId(r.id),
      cardId: learningId(r.cardId),
      rating: r.rating as LearningReview['rating'],
      reviewedAt,
      beforeState,
      afterState,
      undoneAt: nullableTime(r.undoneAt),
    }
  })
  return {
    id,
    fields,
    createdAt: time(v.createdAt),
    updatedAt: time(v.updatedAt),
    deletedAt: nullableTime(v.deletedAt),
    occurrences: list(v.occurrences, 100, '来源数量').map(occurrence),
    tags: normalizeTags(v.tags).sort(utf8Order),
    cards: cards.sort((a, b) => utf8Order(a.direction, b.direction)),
    reviews: reviews.sort((a, b) => a.reviewedAt - b.reviewedAt || utf8Order(a.id, b.id)),
  }
}
export function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`
  if (value && typeof value === 'object')
    return `{${Object.entries(value)
      .filter(([, v]) => v !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
      .map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`)
      .join(',')}}`
  return JSON.stringify(value)
}
export async function digest(value: string): Promise<string> {
  const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(value))
  return Array.from(new Uint8Array(hash), (b) => b.toString(16).padStart(2, '0')).join('')
}
export async function readLearningBackup(file: Blob): Promise<LearningBackup> {
  if (file.size > maxLearningBytes) throw new Error('词句备份不能超过 512 MiB。')
  return validateLearningBackup(JSON.parse(await file.text()))
}
export async function validateLearningBackup(value: unknown): Promise<LearningBackup> {
  const v = object(value)
  requireValue(
    v.format === 'parley-vocabulary' &&
      typeof v.version === 'number' &&
      [1, 2, 3].includes(v.version),
    '文件格式或版本',
  )
  const entries = list(v.entries, 50000, '词句数量').map(validateLearningEntry)
  const ids = new Set<string>()
  let sources = 0,
    reviews = 0
  for (const e of entries) {
    for (const id of [
      e.id,
      ...e.occurrences.map((s) => s.id),
      ...e.cards.map((c) => c.id),
      ...e.reviews.map((r) => r.id),
    ]) {
      requireValue(!ids.has(id), '重复标识')
      ids.add(id)
    }
    const origins = new Set<string>()
    for (const s of e.occurrences) {
      requireValue((await digest(s.snapshot)) === s.snapshotHash, '原文校验')
      const { id: _id, snapshotHash: _hash, ...source } = s
      const fingerprint = canonical(source)
      requireValue(!origins.has(fingerprint), '重复来源')
      origins.add(fingerprint)
      if (v.version === 1) requireValue(!s.backend, 'v1 来源版本')
      if (Number(v.version) >= 2 && s.sourceKind === 'terminal')
        requireValue(
          s.backend &&
            (s.backend.kind === 'claude_code') === s.threadId?.startsWith('claude-code:'),
          '终端命名空间',
        )
    }
    sources += e.occurrences.length
    reviews += e.reviews.length
  }
  requireValue(sources <= 50000 && reviews <= 100000, '来源或复习总数')
  return {
    format: 'parley-vocabulary',
    version: Number(v.version),
    datasetId: learningId(v.datasetId),
    exportedAt: time(v.exportedAt),
    entries,
  }
}
