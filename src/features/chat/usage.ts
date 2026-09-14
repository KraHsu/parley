// Providers report different fields. Keep reported totals; do not infer cache costs
// or treat missing counters as zero.
export function formatUsage(usage: Record<string, unknown> | undefined): string {
  if (!usage) return ''
  const recent = !!usage.last && typeof usage.last === 'object' && !Array.isArray(usage.last)
  const reported = recent ? (usage.last as Record<string, unknown>) : usage
  const count = (...keys: string[]) => {
    for (const key of keys) {
      const value = reported[key]
      if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0)
        return value.toLocaleString('zh-CN')
    }
    return undefined
  }
  const counters = [
    ['输入', count('input_tokens', 'prompt_tokens', 'total_input_tokens', 'inputTokens')],
    ['输出', count('output_tokens', 'completion_tokens', 'total_output_tokens', 'outputTokens')],
    ['合计', count('total_tokens', 'totalTokens')],
    ['缓存读取', count('cache_read_input_tokens', 'cachedInputTokens')],
    ['缓存写入', count('cache_creation_input_tokens', 'cacheWriteInputTokens')],
  ]
    .filter(([, value]) => value !== undefined)
    .map(([label, value]) => `${label} ${value}`)
    .join(' · ')
  return counters ? `${recent ? '最近请求 · ' : ''}${counters} tokens` : ''
}
