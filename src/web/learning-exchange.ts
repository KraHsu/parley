import {
  canonical,
  digest,
  learningId,
  validateLearningBackup,
  validateLearningEntry,
  maxLearningBytes,
  type LearningBackup,
  type LearningEntry,
  type LearningOrigin,
} from '../shared/learning-exchange'
import type { Word } from './types'
import type { Provider } from '../features/backends/types'

// A namespace for Web-origin exports; record UUIDs keep independent workspaces distinct.
const webDataset = 'a74ef028-c29c-487e-9456-29a8e716b074'
export function displayLanguage(entry: LearningEntry) {
  return entry.fields.language === 'und' && entry.fields.languageLabel
    ? entry.fields.languageLabel
    : entry.fields.language
}
function languageFields(value: string) {
  try {
    return { language: Intl.getCanonicalLocales(value)[0] ?? 'und', languageLabel: '' }
  } catch {
    return { language: 'und', languageLabel: value }
  }
}
async function stableId(value: string) {
  const hash = await digest(`parley-learning-id:${value}`)
  return `${hash.slice(0, 8)}-${hash.slice(8, 12)}-4${hash.slice(13, 16)}-8${hash.slice(17, 20)}-${hash.slice(20, 32)}`
}
export function validateOrigin(value: unknown): LearningOrigin {
  if (!value || typeof value !== 'object') throw new Error('词句迁移来源无效。')
  const v = value as LearningOrigin
  if (!/^[a-f0-9]{64}$/.test(v.fingerprint)) throw new Error('词句迁移校验值无效。')
  return {
    datasetId: learningId(v.datasetId),
    recordId: learningId(v.recordId),
    fingerprint: v.fingerprint,
    entry: validateLearningEntry(v.entry),
  }
}
export function syncLearningFields(word: Word) {
  const e = word.learning?.entry
  if (!e) return
  if (word.language !== displayLanguage(e)) Object.assign(e.fields, languageFields(word.language))
  e.fields.text = word.text
  e.fields.meaning = word.meaning
  e.fields.note = word.note
  e.tags = [...word.tags]
  e.updatedAt = Date.now()
}
export async function wordEntry(word: Word, native = 'und'): Promise<LearningEntry> {
  if (word.learning) return validateLearningEntry(word.learning.entry)
  let id: string
  try {
    id = learningId(word.id)
  } catch {
    id = await stableId(word.id)
  }
  const entry: LearningEntry = {
    id,
    fields: {
      ...languageFields(word.language),
      kind: 'word',
      text: word.text,
      meaning: word.meaning,
      meaningLanguage: languageFields(native).language,
      note: word.note,
    },
    createdAt: word.createdAt,
    updatedAt: word.updatedAt ?? word.createdAt,
    deletedAt: null,
    tags: [...word.tags],
    occurrences: [],
    cards: [],
    reviews: [],
  }
  if (word.source?.text) {
    const s = word.source
    const providers: Provider[] = [
      'openai',
      'anthropic',
      'google',
      'deepseek',
      'qwen',
      'kimi',
      'zai',
      'custom',
    ]
    if (s.provider && !providers.includes(s.provider as Provider))
      throw new Error(`“${word.text}”的来源厂商无法识别，请先检查原备份。`)
    entry.occurrences.push({
      id: await stableId(`${id}:source`),
      sourceKind: 'import',
      conversationId: null,
      messageId: null,
      threadId: `parley-web:${s.conversationId}`,
      turnId: null,
      itemId: s.messageId,
      role: 'manual',
      // The old Web format stores a snapshot but no reliable selection offsets.
      // Import the complete passage rather than inventing an original selection.
      selectedText: s.text,
      snapshot: s.text,
      start: 0,
      end: Array.from(s.text).length,
      locatorVersion: 1,
      truncated: false,
      snapshotHash: await digest(s.text),
      ...(s.provider
        ? {
            backend: {
              kind: 'openai_compatible' as const,
              provider: s.provider as Provider,
              model: s.model,
            },
          }
        : {}),
    })
  }
  if (word.review)
    entry.cards.push({
      id: await stableId(`${id}:recognition`),
      entryId: id,
      direction: 'recognition',
      ...word.review,
      suspended: false,
      scheduleVersion: 2,
      revision: 1,
    })
  return validateLearningEntry(entry)
}
export async function exportLearning(words: Word[], native: string): Promise<Blob> {
  const entries: LearningEntry[] = []
  for (const word of words) entries.push(await wordEntry(word, native))
  const backup = await validateLearningBackup({
    format: 'parley-vocabulary',
    version: 3,
    datasetId: webDataset,
    exportedAt: Date.now(),
    entries,
  })
  const blob = new Blob([JSON.stringify(backup)], { type: 'application/json' })
  if (blob.size > maxLearningBytes) throw new Error('导出超过 512 MiB，请分批整理词句后重试。')
  return blob
}
export function entryWord(entry: LearningEntry, origin: Omit<LearningOrigin, 'entry'>): Word {
  entry = JSON.parse(JSON.stringify(entry)) as LearningEntry
  for (const source of entry.occurrences) {
    if (source.sourceKind === 'terminal' && !source.backend) {
      const claude = source.threadId?.startsWith('claude-code:')
      source.backend = {
        kind: claude ? 'claude_code' : 'codex',
        provider: claude ? 'anthropic' : 'openai',
        model: null,
      }
    }
  }
  const first = entry.occurrences[0]
  const recognition = entry.cards.find((c) => c.direction === 'recognition' && !c.suspended)
  return {
    id: entry.id,
    text: entry.fields.text,
    language: displayLanguage(entry),
    meaning: entry.fields.meaning,
    note: entry.fields.note,
    tags: [...entry.tags],
    createdAt: entry.createdAt,
    source: first
      ? {
          conversationId: first.threadId?.startsWith('parley-web:')
            ? first.threadId.slice(11)
            : (first.conversationId ?? ''),
          messageId: first.itemId ?? first.messageId ?? '',
          text: first.snapshot,
          provider: first.backend?.provider ?? '',
          model: first.backend?.model ?? '',
        }
      : null,
    review: recognition
      ? {
          stage: recognition.stage,
          dueAt: recognition.dueAt,
          lastReviewedAt: recognition.lastReviewedAt,
        }
      : null,
    learning: { ...origin, entry: JSON.parse(JSON.stringify(entry)) as LearningEntry },
  }
}
export interface LearningPreview {
  backup: LearningBackup
  signature: string
  added: number
  duplicates: number
  conflicts: number
  trashed: number
  decisions: { entry: LearningEntry; fingerprint: string; kind: 'add' | 'duplicate' | 'conflict' }[]
}
export async function previewLearning(
  backup: LearningBackup,
  words: Word[],
): Promise<LearningPreview> {
  const decisions: LearningPreview['decisions'] = []
  const byId = new Map(words.map((w) => [w.id, w]))
  const byOrigin = new Map<string, Word>()
  const imported = new Set<string>()
  for (const word of words) {
    if (!word.learning) continue
    const identity = `${word.learning.datasetId}:${word.learning.recordId}`
    if (!byOrigin.has(identity)) byOrigin.set(identity, word)
    imported.add(`${identity}:${word.learning.fingerprint}`)
  }
  for (const entry of backup.entries) {
    const fingerprint = await digest(canonical(entry))
    const identity = `${backup.datasetId}:${entry.id}`
    const prior = imported.has(`${identity}:${fingerprint}`)
    const existing = byId.get(entry.id) ?? byOrigin.get(identity)
    const same = existing && canonical(await wordEntry(existing)) === canonical(entry)
    decisions.push({
      entry,
      fingerprint,
      kind: prior || same ? 'duplicate' : existing ? 'conflict' : 'add',
    })
  }
  return {
    backup,
    signature: await digest(canonical(words)),
    decisions,
    added: decisions.filter((d) => d.kind === 'add').length,
    duplicates: decisions.filter((d) => d.kind === 'duplicate').length,
    conflicts: decisions.filter((d) => d.kind === 'conflict').length,
    trashed: backup.entries.filter((e) => e.deletedAt !== null).length,
  }
}
function copyEntry(entry: LearningEntry): LearningEntry {
  const copy = JSON.parse(JSON.stringify(entry)) as LearningEntry
  copy.id = crypto.randomUUID()
  for (const source of copy.occurrences) source.id = crypto.randomUUID()
  const cards = new Map(copy.cards.map((c) => [c.id, crypto.randomUUID()]))
  for (const card of copy.cards) {
    card.id = cards.get(card.id)!
    card.entryId = copy.id
  }
  for (const review of copy.reviews) {
    review.id = crypto.randomUUID()
    review.cardId = cards.get(review.cardId)!
    for (const state of [review.beforeState, review.afterState]) {
      state.id = review.cardId
      state.entryId = copy.id
    }
  }
  return copy
}
export async function mergeLearning(
  preview: LearningPreview,
  words: Word[],
  copyConflicts: boolean,
): Promise<Word[]> {
  if ((await digest(canonical(words))) !== preview.signature)
    throw new Error('词句在预览后发生变化，请重新选择文件。')
  const added = preview.decisions
    .filter((d) => d.kind === 'add' || (copyConflicts && d.kind === 'conflict'))
    .map((d) =>
      entryWord(d.kind === 'conflict' ? copyEntry(d.entry) : d.entry, {
        datasetId: preview.backup.datasetId,
        recordId: d.entry.id,
        fingerprint: d.fingerprint,
      }),
    )
  return [...added, ...words]
}
