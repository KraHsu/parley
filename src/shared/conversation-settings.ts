// Persisted signatures include the last task for compatibility with existing workspaces.
// Task changes belong to a turn, while model/language changes define a conversation.
export function conversationSettings(signature: string): string {
  return signature.replace(/\|(conversation|express|explain|translate)$/, '')
}
