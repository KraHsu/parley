// Shared form/domain rules. Storage adapters remain responsible for validating persisted data.
export function boundedText(value: unknown, max: number, label: string): string {
  if (typeof value !== 'string' || value.includes('\0') || Array.from(value).length > max)
    throw new Error(`${label}最多允许 ${max} 个字符，且不能包含空字符。`)
  return value
}
export function normalizeTags(input: unknown, maxCount = 30, maxLength = 100): string[] {
  if (!Array.isArray(input) || input.length > maxCount)
    throw new Error(`最多允许 ${maxCount} 个标签。`)
  return [
    ...new Set(
      input
        .map((t) => boundedText(t, maxLength, '每个标签').trim().normalize('NFC'))
        .filter(Boolean),
    ),
  ]
}
export function parseTags(input: string, maxCount = 30, maxLength = 100): string[] {
  return normalizeTags(
    input
      .split(/[,，]/)
      .map((s) => s.trim())
      .filter(Boolean),
    maxCount,
    maxLength,
  )
}
export function sameLearningText(a: string, b: string): boolean {
  return a.trim().normalize('NFC') === b.trim().normalize('NFC')
}
