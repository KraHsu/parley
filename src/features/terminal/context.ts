export interface TerminalThread {
  id: string
  title: string
  createdAt: number
}
export interface TerminalMessage {
  id: string
  role: 'user' | 'assistant'
  text: string
}
export interface TerminalSnapshot {
  threads: TerminalThread[]
  messages: TerminalMessage[]
}
export function autoSelectThread(threads: TerminalThread[], startedAt: number): string {
  if (!startedAt) return ''
  const fresh = threads.filter((thread) => thread.createdAt >= startedAt)
  // Never guess when several CLI sessions started together.
  return fresh.length === 1 ? fresh[0]!.id : ''
}
export function studyContext(messages: TerminalMessage[], selection = ''): string {
  const recent = messages
    .slice(-6)
    .map((message) => `${message.role}: ${message.text}`)
    .join('\n\n')
  const text = selection ? `${recent}\n\nSelected passage for study: ${selection}` : recent
  return Array.from(text).slice(-5000).join('')
}
