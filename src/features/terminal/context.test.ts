import { describe, expect, it } from 'vitest'
import { autoSelectThread, studyContext } from './context'
describe('terminal context', () => {
  it('only automatically links a unique session created by this launch', () => {
    const old = { id: 'old', title: 'Old', createdAt: 90 }
    const current = { id: 'current', title: 'Current', createdAt: 101 }
    expect(autoSelectThread([old, current], 100)).toBe('current')
    expect(autoSelectThread([old], 100)).toBe('')
    expect(autoSelectThread([current], 0)).toBe('')
    expect(autoSelectThread([current, { ...current, id: 'other' }], 100)).toBe('')
  })
  it('keeps six recent conversational rounds', () => {
    const messages = Array.from({ length: 16 }, (_, i) => ({
      id: String(i),
      role: i % 2 ? ('assistant' as const) : ('user' as const),
      text: `message-${i}`,
    }))
    const text = studyContext(messages)
    expect(text).not.toContain('message-3\n')
    expect(text).toContain('user: message-4')
    expect(text).toContain('assistant: message-15')
  })
  it('includes the selected passage and recent dialog within the IPC byte limit', () => {
    const text = studyContext([{ id: '1', role: 'assistant', text: '😀'.repeat(8000) }], 'a phrase')
    expect(text).toContain('Selected passage for study: a phrase')
    expect(new TextEncoder().encode(text).length).toBeLessThan(24000)
    expect(text).not.toContain('\ufffd')
  })
})
