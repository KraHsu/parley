// Providers report different fields. Keep reported totals; do not infer cache costs
// or treat missing counters as zero.
export function formatUsage(usage: Record<string, unknown> | undefined): string {
  if (!usage) return ''
  const count = (...keys: string[]) => {
    for (const key of keys) {
      const value = usage[key]
      if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0)
        return value.toLocaleString('zh-CN')
    }
    return undefined
  }
  const counters = [
    ['输入', count('input_tokens', 'prompt_tokens', 'total_input_tokens')],
    ['输出', count('output_tokens', 'completion_tokens', 'total_output_tokens')],
    ['合计', count('total_tokens')],
    ['缓存读取', count('cache_read_input_tokens')],
    ['缓存写入', count('cache_creation_input_tokens')],
  ]
    .filter(([, value]) => value !== undefined)
    .map(([label, value]) => `${label} ${value}`)
    .join(' · ')
  return counters ? `${counters} tokens` : ''
}
