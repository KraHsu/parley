import { describe, expect, it } from 'vitest'
import { captureText, type SourceOrigin } from './selection'
const origin: SourceOrigin = {
  sourceKind: 'terminal',
  conversationId: null,
  messageId: null,
  threadId: 'thread',
  turnId: 'turn',
  itemId: 'item',
  role: 'assistant',
  truncated: false,
}
describe('learning selection', () => {
  it('uses scalar offsets for emoji and finds the second repeated phrase', () => {
    const text = '🌍 café and café'
    const at = text.lastIndexOf('café')
    const result = captureText(text, at, text.length, origin)
    expect(result.start).toBe(11)
    expect(result.end).toBe(15)
    expect(result.selectedText).toBe('café')
  })
  it('rejects splitting a surrogate or combining sequence', () => {
    expect(() => captureText('🌍', 0, 1, origin)).toThrow()
    expect(() => captureText('cafe\u0301', 0, 4, origin)).toThrow()
    expect(captureText('cafe\u0301', 0, 5, origin).end).toBe(5)
  })
  it('keeps CJK and RTL exactly as selected', () => {
    for (const text of ['你好世界', 'مرحبا بك'])
      expect(captureText(text, 0, text.length, origin).selectedText).toBe(text)
  })
  it('bounds a long snapshot without dropping the selected material', () => {
    const text = '🌍'.repeat(20000) + '你好' + 'é'.repeat(20000)
    const start = 40000
    const result = captureText(text, start, start + 2, origin)
    expect(result.truncated).toBe(true)
    expect(Array.from(result.snapshot).length).toBeLessThanOrEqual(16000)
    expect(Array.from(result.snapshot).slice(result.start, result.end).join('')).toBe('你好')
  })
  it('rejects overlong selections rather than silently truncating the term', () => {
    expect(() => captureText('a'.repeat(2001), 0, 2001, origin)).toThrow('2,000')
  })
})
