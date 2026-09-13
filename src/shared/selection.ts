import type { VocabularySource } from '../features/vocabulary/types'
export type SourceOrigin = Pick<
  VocabularySource,
  | 'sourceKind'
  | 'conversationId'
  | 'messageId'
  | 'threadId'
  | 'turnId'
  | 'itemId'
  | 'role'
  | 'truncated'
>
export function captureText(
  text: string,
  start16: number,
  end16: number,
  origin: SourceOrigin,
): VocabularySource {
  const boundaries = new Set<number>([text.length])
  const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' })
  for (const segment of segmenter.segment(text)) boundaries.add(segment.index)
  if (start16 >= end16 || !boundaries.has(start16) || !boundaries.has(end16))
    throw new Error('请选择完整的词句，不要拆开组合字符。')
  const selectedText = text.slice(start16, end16)
  if (!selectedText.trim()) throw new Error('请选择要学习的文字。')
  if (Array.from(selectedText).length > 2000) throw new Error('词句最多 2,000 个字符，请缩小选区。')
  // Keep a bounded source excerpt around the selection, on grapheme boundaries.
  let snapshot = text,
    start = start16,
    end = end16
  if (Array.from(text).length > 16000) {
    const scalars = Array.from(text)
    const scalarStart = Array.from(text.slice(0, start16)).length
    const from = Math.max(0, scalarStart - 2000)
    let left = scalars.slice(0, from).join('').length
    let right = scalars.slice(0, Math.min(scalars.length, from + 15000)).join('').length
    while (!boundaries.has(left)) left++
    while (!boundaries.has(right)) right--
    if (left > start16 || right < end16) throw new Error('请选择较短的词句。')
    snapshot = text.slice(left, right)
    start -= left
    end -= left
  }
  return {
    ...origin,
    selectedText,
    snapshot,
    start: Array.from(snapshot.slice(0, start)).length,
    end: Array.from(snapshot.slice(0, end)).length,
    locatorVersion: 1,
    truncated: origin.truncated || snapshot !== text,
  }
}
export function captureSelection(
  element: HTMLElement,
  origin: SourceOrigin,
): VocabularySource | null {
  const selection = window.getSelection()
  if (!selection?.rangeCount || selection.isCollapsed) return null
  const range = selection.getRangeAt(0)
  if (!element.contains(range.startContainer) || !element.contains(range.endContainer))
    throw new Error('请在同一条消息中选择词句。')
  const before = document.createRange()
  before.selectNodeContents(element)
  before.setEnd(range.startContainer, range.startOffset)
  const start = before.toString().length
  return captureText(element.textContent ?? '', start, start + range.toString().length, origin)
}
