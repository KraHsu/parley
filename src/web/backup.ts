import { initialState, type WebState, type Word } from './types'
import { validateState, limits } from './validation'

// v1 JSON remains readable. New files use one bounded record per line, so file size
// is not limited by a single JSON string or an arbitrary total-byte threshold.
export function exportState(state: WebState): string {
  return JSON.stringify(
    { app: 'parley-web', exportedAt: new Date().toISOString(), state: validateState(state) },
    null,
    2,
  )
}
export function importState(raw: string): WebState {
  const input = JSON.parse(raw)
  if (input.app !== 'parley-web')
    throw new Error('请选择 Parley Web 工作区备份；词句 JSON 请在词句页的迁移入口导入。')
  return validateState(input.state)
}
export function exportBackup(state: WebState): Blob {
  const clean = validateState(state)
  const parts: string[] = []
  const line = (value: unknown) => {
    const serialized = JSON.stringify(value)
    if (serialized.length > 32 * 1024 * 1024) throw new Error('备份单条记录过大。')
    parts.push(serialized + '\n')
  }
  let learningSources = 0,
    learningReviews = 0
  line({ app: 'parley-web', format: 3, settings: clean.settings, active: clean.active })
  for (const p of clean.profiles) line({ kind: 'profile', value: p })
  for (const c of clean.conversations) {
    line({ kind: 'conversation', value: { ...c, messages: [] } })
    for (const message of c.messages)
      line({ kind: 'message', conversationId: c.id, value: message })
  }
  for (const word of clean.words) {
    if (!word.learning) {
      line({ kind: 'word', value: word })
      continue
    }
    const { occurrences, reviews, ...entry } = word.learning.entry
    line({
      kind: 'word',
      value: {
        ...word,
        learning: { ...word.learning, entry: { ...entry, occurrences: [], reviews: [] } },
      },
    })
    for (const source of occurrences) {
      line({ kind: 'learning-source', wordId: word.id, value: source })
      learningSources++
    }
    for (const review of reviews) {
      line({ kind: 'learning-review', wordId: word.id, value: review })
      learningReviews++
    }
  }
  line({
    kind: 'end',
    learningSources,
    learningReviews,
    profiles: clean.profiles.length,
    conversations: clean.conversations.length,
    words: clean.words.length,
    messages: clean.conversations.reduce((n, c) => n + c.messages.length, 0),
  })
  return new Blob(parts, { type: 'application/x-ndjson' })
}
export async function readBackup(file: Blob): Promise<WebState> {
  const reader = file
    .stream()
    .pipeThrough(new TextDecoderStream('utf-8', { fatal: true }))
    .getReader()
  const state = initialState()
  state.profiles = []
  state.conversations = []
  state.words = []
  const conversations = new Map<string, WebState['conversations'][number]>()
  const words = new Map<string, Word>()
  let format = 2,
    learningSources = 0,
    learningReviews = 0
  let buffer = '',
    header = false,
    ended = false,
    messages = 0
  function accept(line: string) {
    if (!line.trim()) return
    if (ended) throw new Error('备份结束后还有额外记录。')
    const row = JSON.parse(line)
    if (!header) {
      if (row.app !== 'parley-web' || ![2, 3].includes(row.format))
        throw new Error('Web 备份格式无效。')
      format = row.format
      state.settings = row.settings
      state.active = row.active
      header = true
    } else if (row.kind === 'profile') state.profiles.push(row.value)
    else if (row.kind === 'conversation') {
      if (conversations.has(row.value.id) || row.value.messages?.length !== 0)
        throw new Error('备份包含重复或无效会话。')
      state.conversations.push(row.value)
      conversations.set(row.value.id, row.value)
    } else if (row.kind === 'message') {
      const c = conversations.get(row.conversationId)
      if (!c || c.messages.length >= limits.messages)
        throw new Error('备份消息的会话无效或数量超限。')
      c.messages.push(row.value)
      messages++
    } else if (row.kind === 'word') {
      if (words.has(row.value.id)) throw new Error('备份包含重复词句。')
      if (
        format === 3 &&
        row.value.learning &&
        (row.value.learning.entry.occurrences?.length !== 0 ||
          row.value.learning.entry.reviews?.length !== 0)
      )
        throw new Error('词句迁移记录必须逐行保存。')
      state.words.push(row.value)
      words.set(row.value.id, row.value)
    } else if (row.kind === 'learning-source' || row.kind === 'learning-review') {
      const entry = words.get(row.wordId)?.learning?.entry
      if (format !== 3 || !entry) throw new Error('迁移记录缺少所属词句。')
      if (row.kind === 'learning-source') {
        if (entry.occurrences.length >= 100) throw new Error('词句来源数量超限。')
        entry.occurrences.push(row.value)
        learningSources++
      } else {
        if (entry.reviews.length >= 100000) throw new Error('词句复习记录数量超限。')
        entry.reviews.push(row.value)
        learningReviews++
      }
    } else if (row.kind === 'end') {
      if (
        row.profiles !== state.profiles.length ||
        row.conversations !== state.conversations.length ||
        row.words !== state.words.length ||
        row.messages !== messages ||
        (format === 3 &&
          (row.learningSources !== learningSources || row.learningReviews !== learningReviews))
      )
        throw new Error('备份记录数量不完整。')
      ended = true
    } else throw new Error('备份包含未知记录。')
    if (
      state.words.length > limits.words ||
      state.profiles.length > limits.profiles ||
      state.conversations.length > limits.conversations
    )
      throw new Error('备份记录数量超出支持范围。')
  }
  try {
    while (true) {
      const { value, done } = await reader.read()
      if (done) break
      buffer += value
      // Pretty-printed v1 JSON starts with "{" on its own line; compact v1 is
      // detected from its top-level app/state header. It remains backward-compatible.
      if (!header && buffer.includes('\n')) {
        const first = buffer.slice(0, buffer.indexOf('\n')).trim()
        let lines = false
        try {
          lines = [2, 3].includes(JSON.parse(first).format)
        } catch {
          /* legacy JSON */
        }
        if (!lines) {
          await reader.cancel()
          return importState(await file.text())
        }
      }
      let newline: number
      while ((newline = buffer.indexOf('\n')) >= 0) {
        if (newline > 32 * 1024 * 1024) throw new Error('备份单条记录过大。')
        accept(buffer.slice(0, newline))
        buffer = buffer.slice(newline + 1)
      }
      if (buffer.length > 32 * 1024 * 1024) {
        if (!header) {
          await reader.cancel()
          return importState(await file.text())
        }
        throw new Error('备份单条记录过大。')
      }
    }
    if (!header) return importState(buffer)
    if (buffer.trim()) accept(buffer)
    if (!ended) throw new Error('备份不完整，缺少结束记录。')
    return validateState(state)
  } finally {
    await reader.cancel().catch(() => {})
    reader.releaseLock()
  }
}
