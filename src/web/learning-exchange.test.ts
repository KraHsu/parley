import { readFileSync, writeFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import {
  readLearningBackup,
  validateLearningBackup,
  canonical,
  digest,
} from '../shared/learning-exchange'
import {
  entryWord,
  exportLearning,
  mergeLearning,
  previewLearning,
  wordEntry,
} from './learning-exchange'
import { createLearning } from './learning'
import { initialState, type Word } from './types'
import { exportBackup, readBackup } from './backup'
import { createPersistence } from './persistence'
const fixture = () => JSON.parse(readFileSync('fixtures/learning-exchange.json', 'utf8'))
const words = async () => {
  const backup = await validateLearningBackup(fixture())
  return mergeLearning(await previewLearning(backup, []), [], false)
}
describe('learning exchange between desktop and Web', () => {
  it('preserves multiple sources, both card directions, history and trash through Web and workspace backup', async () => {
    const state = initialState()
    state.words = await words()
    expect(state.words[0]!.review?.stage).toBe(6)
    const restored = await readBackup(exportBackup(state))
    const exported = await readLearningBackup(await exportLearning(restored.words, 'zh-CN'))
    expect(canonical(exported.entries)).toBe(canonical(fixture().entries))
  })
  it('keeps imports idempotent, preserves local edits and remaps all IDs for conflict copies', async () => {
    const backup = await validateLearningBackup(fixture()),
      original = await words()
    expect((await previewLearning(backup, original)).duplicates).toBe(2)
    const changed = await validateLearningBackup({
      ...fixture(),
      entries: fixture().entries.map((e: object) => structuredClone(e)),
    })
    changed.entries[0]!.fields.note = 'different note'
    const preview = await previewLearning(changed, original)
    expect(preview.conflicts).toBe(1)
    expect((await mergeLearning(preview, original, false)).length).toBe(2)
    const copied = await mergeLearning(preview, original, true)
    expect(copied).toHaveLength(3)
    expect(copied[0]!.id).not.toBe(original[0]!.id)
    await readLearningBackup(await exportLearning(copied, 'zh-CN'))
    expect((await previewLearning(changed, copied)).duplicates).toBe(2)
    original[0]!.note = 'edited after preview'
    await expect(mergeLearning(preview, original, true)).rejects.toThrow('预览后')
  })
  it('exports ordinary Web words including stage 6 and unrecognized language labels without truncation', async () => {
    const word: Word = {
      id: 'legacy-id',
      text: 'x'.repeat(10000),
      language: '日本語',
      meaning: 'meaning',
      note: 'n'.repeat(16000),
      tags: Array.from({ length: 30 }, (_, i) => `${i}`.padEnd(100, 'é')),
      source: {
        conversationId: 'old',
        messageId: 'message',
        text: '🌍'.repeat(20000),
        provider: 'kimi',
        model: 'fixture',
      },
      createdAt: 1,
      review: { stage: 6, dueAt: 123, lastReviewedAt: 12 },
    }
    const entry = await wordEntry(word, '中文')
    const round = entryWord(entry, {
      datasetId: fixture().datasetId,
      recordId: entry.id,
      fingerprint: 'a'.repeat(64),
    })
    expect(round.language).toBe('日本語')
    expect(round.source).toEqual(word.source)
    expect(round.review).toEqual(word.review)
    expect(round.text).toBe(word.text)
    expect(round.tags).toEqual([...word.tags].sort())
    expect((await readLearningBackup(await exportLearning([word], '中文'))).entries[0]).toEqual(
      entry,
    )
  })
  it('keeps desktop version-1 scheduling and records valid review history when practiced in Web', async () => {
    const state = initialState()
    state.words = await words()
    const entry = state.words[0]!.learning!.entry
    const recognition = entry.cards.find((c) => c.direction === 'recognition')!
    recognition.scheduleVersion = 1
    recognition.stage = 5
    // Start an independent valid v1 card without a stale v2 history.
    entry.reviews = entry.reviews.filter((r) => r.cardId !== recognition.id)
    const persistence = createPersistence(state)
    const learning = createLearning(state, persistence, () => true)
    learning.review(state.words[0]!, true)
    persistence.dispose()
    expect(recognition.stage).toBe(5)
    expect(recognition.dueAt - recognition.lastReviewedAt!).toBe(30 * 86400000)
    await readLearningBackup(await exportLearning(state.words, 'zh-CN'))
  })
  it('rejects corrupt sources, duplicate global IDs, invalid schedules and future formats before merging', async () => {
    let data = fixture()
    data.entries[0].occurrences[0].snapshotHash = '0'.repeat(64)
    await expect(validateLearningBackup(data)).rejects.toThrow('原文校验')
    data = fixture()
    data.entries[1].id = data.entries[0].id
    await expect(validateLearningBackup(data)).rejects.toThrow('重复标识')
    data = fixture()
    data.entries[0].reviews[0].afterState.dueAt++
    await expect(validateLearningBackup(data)).rejects.toThrow('排程')
    data = fixture()
    data.version = 99
    await expect(validateLearningBackup(data)).rejects.toThrow('格式或版本')
  })
})

it('exports a runtime-generated Web file for the Rust importer when requested', async () => {
  if (!process.env.PARLEY_TEST_LEARNING_EXPORT) return
  const state = initialState()
  state.words = [
    {
      id: crypto.randomUUID(),
      text: 'web → desktop 🌍',
      language: '日本語',
      meaning: 'm'.repeat(10000),
      note: 'n'.repeat(16000),
      tags: Array.from({ length: 30 }, (_, i) => `${i}`.padEnd(100, 'é')),
      source: {
        conversationId: 'web',
        messageId: 'message',
        text: '🌍'.repeat(20000),
        provider: 'kimi',
        model: 'fixture',
      },
      createdAt: 1700000000000,
      review: { stage: 6, dueAt: 1800000000000, lastReviewedAt: 1700000000000 },
    },
  ]
  writeFileSync(
    process.env.PARLEY_TEST_LEARNING_EXPORT,
    await (await exportLearning(state.words, '中文')).text(),
  )
})

it('upgrades legacy terminal sources when exporting v1 imports as v3', async () => {
  const legacy = fixture()
  legacy.version = 1
  for (const entry of legacy.entries) for (const source of entry.occurrences) delete source.backend
  const backup = await validateLearningBackup(legacy)
  const words = await mergeLearning(await previewLearning(backup, []), [], false)
  const exported = await readLearningBackup(await exportLearning(words, 'zh-CN'))
  expect(exported.entries[0]!.occurrences[1]!.backend?.kind).toBe('claude_code')
})

it('splits a large migrated word into bounded workspace backup records and detects missing sources', async () => {
  const backup = await validateLearningBackup(fixture()),
    entry = backup.entries[0]!
  entry.occurrences = []
  for (let i = 0; i < 5; i++) {
    const snapshot = `${i}${'x'.repeat(1_999_999)}`
    entry.occurrences.push({
      ...fixture().entries[0].occurrences[0],
      id: crypto.randomUUID(),
      snapshot,
      selectedText: snapshot,
      start: 0,
      end: snapshot.length,
      snapshotHash: await digest(snapshot),
    })
  }
  const state = initialState()
  state.words = [
    entryWord(entry, {
      datasetId: backup.datasetId,
      recordId: entry.id,
      fingerprint: 'c'.repeat(64),
    }),
  ]
  const blob = exportBackup(state)
  expect(blob.size).toBeGreaterThan(16 * 1024 * 1024)
  const restored = await readBackup(blob)
  expect(restored.words[0]!.learning!.entry.occurrences).toEqual(entry.occurrences)
  const lines = (await blob.text()).split('\n')
  lines.splice(
    lines.findIndex((line) => line.includes('"kind":"learning-source"')),
    1,
  )
  await expect(readBackup(new Blob([lines.join('\n')]))).rejects.toThrow('记录数量不完整')
})
