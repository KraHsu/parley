import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope } from 'vue'
import { initialState } from './types'
const mocks = vi.hoisted(() => ({ save: vi.fn(), generate: vi.fn() }))
vi.mock('./storage', async (original) => ({
  ...(await original<typeof import('./storage')>()),
  loadState: async () => initialState(),
  saveState: mocks.save,
}))
vi.mock('./api', async (original) => ({
  ...(await original<typeof import('./api')>()),
  generate: mocks.generate,
}))
import { createWebWorkspace } from './store'
let scope = effectScope()
beforeEach(() => {
  vi.stubGlobal('navigator', {})
  mocks.save.mockReset().mockResolvedValue(undefined)
  mocks.generate.mockReset()
  scope = effectScope()
})
afterEach(() => {
  scope.stop()
  vi.unstubAllGlobals()
})
async function workspace() {
  const w = scope.run(() => createWebWorkspace())!
  await w.initialize()
  w.saveProfile(
    {
      id: 'api',
      name: 'API',
      kind: 'openai_responses',
      provider: 'openai',
      endpoint: 'https://api.example.test/v1',
    },
    'private-key',
  )
  w.selectModel('main', 'model')
  w.selectModel('tutor', 'model')
  return w
}
describe('web workspace routing', () => {
  it('keeps two panes independent while canceling one and preserving partial text', async () => {
    const w = await workspace()
    let releaseTutor: (() => void) | undefined
    mocks.generate.mockImplementation(
      async (_profile, conversation, _key, _input, _mode, signal, changed) => {
        changed({ text: 'partial', complete: false })
        if (conversation.pane === 'main')
          await new Promise((_, reject) =>
            signal.addEventListener('abort', () => reject(new Error('aborted')), { once: true }),
          )
        else {
          await new Promise<void>((resolve) => {
            releaseTutor = resolve
          })
          changed({
            text: 'Tutor complete',
            complete: true,
            continuation: [],
            usage: { total_tokens: 8 },
          })
        }
        return false
      },
    )
    const main = w.send('main', 'main question'),
      tutor = w.send('tutor', 'tutor question')
    await vi.waitFor(() => expect(mocks.generate).toHaveBeenCalledTimes(2))
    w.stop('main')
    await main
    expect(w.lane('main').messages.at(-1)?.status).toBe('interrupted')
    expect(w.lane('main').messages.at(-1)?.text).toBe('partial')
    expect(w.busy.tutor).toBe(true)
    releaseTutor!()
    await tutor
    expect(w.lane('tutor').messages.at(-1)?.status).toBe('complete')
    expect(w.lane('tutor').messages.at(-1)?.text).toBe('Tutor complete')
    expect(JSON.stringify(mocks.save.mock.calls)).not.toContain('private-key')
    w.dispose()
  })
  it('keeps history on backend/model change and blocks a paid request when saving fails', async () => {
    const w = await workspace()
    mocks.generate.mockResolvedValue(false)
    w.lane('main').messages.push({ id: 'old', role: 'user', text: 'old text', status: 'complete' })
    w.lane('main').draft = 'draft'
    const old = w.lane('main').id
    w.selectModel('main', 'new-model')
    expect(w.lane('main').id).not.toBe(old)
    expect(w.lane('main').draft).toBe('draft')
    expect(w.state.conversations.find((c) => c.id === old)?.messages[0]?.text).toBe('old text')
    mocks.save.mockRejectedValue(new Error('disk full'))
    await w.send('main', 'paid question')
    expect(mocks.generate).not.toHaveBeenCalled()
    expect(w.storageError).toBe('disk full')
    expect(w.lane('main').messages.at(-1)?.status).toBe('failed')
    w.dispose()
  })
  it('updates unused languages and keeps word snapshots after deleting their source', async () => {
    const w = await workspace()
    w.state.settings.target = 'fr'
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(w.lane('main').target).toBe('fr')
    const c = w.lane('main')
    c.messages.push({ id: 'original', role: 'assistant', text: 'Bonjour', status: 'complete' })
    await w.saveWord({
      id: 'word',
      text: 'Bonjour',
      language: 'fr',
      meaning: '你好',
      note: '',
      tags: [],
      createdAt: 1,
      review: null,
      source: {
        conversationId: c.id,
        messageId: 'original',
        text: 'Bonjour',
        provider: 'openai',
        model: 'model',
      },
    })
    w.removeProfile('api')
    expect(w.state.profiles).toHaveLength(1)
    w.removeConversation('main')
    expect(w.state.words[0]?.source?.text).toBe('Bonjour')
    expect(w.state.conversations.some((item) => item.id === c.id)).toBe(false)
    w.removeProfile('api')
    expect(w.state.profiles).toHaveLength(0)
    expect(w.lane('main').profileId).toBe('')
    expect(w.keys.has('api')).toBe(false)
    w.dispose()
  })
})

it('keeps the current workspace and credentials when restoring cannot commit', async () => {
  const w = await workspace()
  await w.persist()
  const original = JSON.parse(JSON.stringify(w.state))
  const backup = initialState()
  backup.settings.target = 'ja'
  mocks.save.mockRejectedValue(new Error('disk full'))
  expect(await w.restore(backup)).toBe(false)
  expect(w.state).toEqual(original)
  expect(w.keys.get('api')).toBe('private-key')
  expect(w.notice).not.toContain('备份已恢复')
  expect(w.error).toBe('disk full')
  expect(w.canEdit).toBe(true)
  w.dispose()
})
it('rejects an oversized tag before changing state and does not report success before a durable save', async () => {
  const w = await workspace()
  const word = {
    id: 'word',
    text: 'hello',
    language: 'en',
    meaning: '',
    note: '',
    tags: ['x'.repeat(101)],
    source: null,
    createdAt: 1,
    review: null,
  }
  await expect(w.saveWord(word)).rejects.toThrow('100')
  expect(w.state.words).toHaveLength(0)
  word.tags = ['daily']
  mocks.save.mockRejectedValue(new Error('disk full'))
  await expect(w.saveWord(word)).rejects.toThrow('disk full')
  expect(w.dirty).toBe(true)
  w.dispose()
})
