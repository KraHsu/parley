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
  it('includes the selected passage and recent dialog within the IPC byte limit', () => {
    const text = studyContext([{ id: '1', role: 'assistant', text: '😀'.repeat(8000) }], 'a phrase')
    expect(text).toContain('Selected passage for study: a phrase')
    expect(new TextEncoder().encode(text).length).toBeLessThan(24000)
    expect(text).not.toContain('\ufffd')
  })
})
