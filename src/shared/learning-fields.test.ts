import { expect, it } from 'vitest'
import { parseTags, sameLearningText } from './learning-fields'
it('uses canonical Unicode equivalence while retaining case and compatibility distinctions', () => {
  expect(sameLearningText(' café ', 'cafe\u0301')).toBe(true)
  expect(sameLearningText('US', 'us')).toBe(false)
  expect(sameLearningText('Ａ', 'A')).toBe(false)
  expect(parseTags('café, cafe\u0301，daily')).toEqual(['café', 'daily'])
})
it('validates individual tags and adapter limits without silently dropping tags', () => {
  expect(() => parseTags('x'.repeat(101))).toThrow('100')
  expect(() => parseTags('x'.repeat(51), 20, 50)).toThrow('50')
  expect(() => parseTags(Array.from({ length: 31 }, (_, i) => `tag-${i}`).join(','))).toThrow('30')
})
