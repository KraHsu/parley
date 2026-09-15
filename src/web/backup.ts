import { initialState, type WebState } from './types'
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
    throw new Error('请选择 Parley Web 备份；桌面备份请在桌面端导入。')
  return validateState(input.state)
}
export function exportBackup(state: WebState): Blob {
  const clean = validateState(state)
  const parts: string[] = []
  const line = (value: unknown) => parts.push(JSON.stringify(value) + '\n')
  line({ app: 'parley-web', format: 2, settings: clean.settings, active: clean.active })
  for (const p of clean.profiles) line({ kind: 'profile', value: p })
  for (const c of clean.conversations) {
    line({ kind: 'conversation', value: { ...c, messages: [] } })
    for (const message of c.messages)
      line({ kind: 'message', conversationId: c.id, value: message })
  }
  for (const w of clean.words) line({ kind: 'word', value: w })
  line({
    kind: 'end',
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
  let buffer = '',
    header = false,
    ended = false,
    messages = 0
  function accept(line: string) {
    if (!line.trim()) return
    if (ended) throw new Error('备份结束后还有额外记录。')
    const row = JSON.parse(line)
    if (!header) {
      if (row.app !== 'parley-web' || row.format !== 2) throw new Error('Web 备份格式无效。')
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
    } else if (row.kind === 'word') state.words.push(row.value)
    else if (row.kind === 'end') {
      if (
        row.profiles !== state.profiles.length ||
        row.conversations !== state.conversations.length ||
        row.words !== state.words.length ||
        row.messages !== messages
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
        let v2 = false
        try {
          v2 = JSON.parse(first).format === 2
        } catch {
          /* legacy JSON */
        }
        if (!v2) {
          await reader.cancel()
          return importState(await file.text())
        }
      }
      let newline: number
      while ((newline = buffer.indexOf('\n')) >= 0) {
        if (newline > 16 * 1024 * 1024) throw new Error('备份单条记录过大。')
        accept(buffer.slice(0, newline))
        buffer = buffer.slice(newline + 1)
      }
      if (buffer.length > 16 * 1024 * 1024) {
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
