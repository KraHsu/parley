import { describe, expect, it } from 'vitest'
import { formatUsage } from './usage'

describe('reported token usage', () => {
  it('handles native and compatible fields without estimating absent totals', () => {
    expect(formatUsage({ input_tokens: 20, output_tokens: 3, total_tokens: 23 })).toBe(
      '输入 20 · 输出 3 · 合计 23 tokens',
    )
    expect(formatUsage({ prompt_tokens: 20, completion_tokens: 3 })).toBe('输入 20 · 输出 3 tokens')
    expect(formatUsage({ total_input_tokens: 20, total_output_tokens: 3, total_tokens: 25 })).toBe(
      '输入 20 · 输出 3 · 合计 25 tokens',
    )
    expect(
      formatUsage({
        input_tokens: 20,
        cache_read_input_tokens: 10,
        cache_creation_input_tokens: 4,
      }),
    ).toBe('输入 20 · 缓存读取 10 · 缓存写入 4 tokens')
  })
  it('labels Codex last-request counters and preserves thread totals separately', () => {
    expect(
      formatUsage({
        last: { inputTokens: 20, outputTokens: 3, totalTokens: 23, cachedInputTokens: 4 },
        total: { inputTokens: 1000, totalTokens: 2000 },
      }),
    ).toBe('最近请求 · 输入 20 · 输出 3 · 合计 23 · 缓存读取 4 tokens')
  })
  it('distinguishes reported zero from missing, invalid, and unknown counters', () => {
    expect(formatUsage({ input_tokens: 0 })).toBe('输入 0 tokens')
    for (const value of [
      undefined,
      {},
      { total_tokens: -1 },
      { total_tokens: '12' },
      { total_tokens: Infinity },
      { total_tokens: 1.2 },
      { secret: 'do not render' },
    ])
      expect(formatUsage(value)).toBe('')
  })
})
