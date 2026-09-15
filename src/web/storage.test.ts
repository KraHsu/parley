import { describe, expect, it } from 'vitest'
import { exportState, importState, validateState } from './storage'
import { initialState } from './types'
describe('web backups', () => {
  it('round-trips learning data and drops credentials, opaque provider data and active generation', () => {
    const state = initialState()
    Object.assign(state, { keys: { test: 'private-key' } })
    state.conversations[0]!.messages.push(
      { id: 'u', role: 'user', text: 'Hi', status: 'complete', input: 'private-request' },
      {
        id: 'a',
        role: 'assistant',
        text: 'partial',
        status: 'streaming',
        continuation: { secret: 'private-key' },
      },
    )
    state.words.push({
      id: 'word',
      text: 'Hi',
      language: 'en',
      meaning: '你好',
      note: 'greeting',
      tags: ['daily'],
      source: {
        conversationId: state.active.main,
        messageId: 'u',
        text: 'Hi',
        provider: 'openai',
        model: 'fixture',
      },
      createdAt: 10,
      review: { stage: 2, dueAt: 100, lastReviewedAt: 50 },
    })
    const raw = exportState(state),
      restored = importState(raw)
    expect(raw).not.toContain('private-key')
    expect(raw).not.toContain('private-request')
    expect(restored.words).toEqual(state.words)
    expect(restored.conversations[0]!.messages[1]!.status).toBe('interrupted')
    expect(importState(exportState(restored))).toEqual(restored)
  })
  it('rejects desktop or unknown backups, dangling active IDs, duplicate IDs and invalid review states', () => {
    expect(() => importState('{"version":2,"entries":[]}')).toThrow()
    const state = initialState()
    state.active.main = 'missing'
    expect(() => validateState(state)).toThrow()
    const duplicate = initialState()
    duplicate.conversations.push(duplicate.conversations[0]!)
    expect(() => validateState(duplicate)).toThrow()
    const cli = initialState()
    Object.assign(cli, {
      profiles: [{ id: 'codex', kind: 'codex', provider: 'openai', name: 'Codex', endpoint: '' }],
    })
    expect(() => validateState(cli)).toThrow()
  })
})

it('round-trips a valid backup larger than the former 32 MiB limit in both formats', async () => {
  const { exportBackup, readBackup } = await import('./backup')
  const state = initialState()
  state.words = Array.from({ length: 1700 }, (_, i) => ({
    id: `word-${i}`,
    text: 'example',
    language: 'en',
    meaning: '词'.repeat(7000),
    note: '',
    tags: [],
    source: null,
    createdAt: 1,
    review: null,
  }))
  const legacy = exportState(state)
  expect(new TextEncoder().encode(legacy).length).toBeGreaterThan(32 * 1024 * 1024)
  expect(importState(legacy).words).toHaveLength(1700)
  const backup = exportBackup(state)
  expect(backup.size).toBeGreaterThan(32 * 1024 * 1024)
  expect((await readBackup(backup)).words).toEqual(state.words)
  await expect(readBackup(backup.slice(0, backup.size - 100))).rejects.toThrow()
})
