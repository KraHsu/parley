import { ref } from 'vue'
import { emptyChanges, mergeChanges, saveState, type Changes } from './storage'
import type { WebState } from './types'

export function createPersistence(state: WebState) {
  const dirty = ref(false),
    saving = ref(false),
    storageError = ref('')
  let pending = emptyChanges(),
    full = false,
    timer: ReturnType<typeof setTimeout> | undefined
  let task: Promise<boolean> | undefined
  function mark(kind: 'meta' | 'catalog' | 'profiles' | 'conversations' | 'words', id?: string) {
    if (kind === 'meta' || kind === 'catalog') pending[kind] = true
    else pending[kind].add(id!)
    schedule()
  }
  function message(conversation: string, id: string) {
    if (!pending.messages.has(conversation)) pending.messages.set(conversation, new Set())
    pending.messages.get(conversation)!.add(id)
    schedule()
  }
  function schedule() {
    dirty.value = true
    clearTimeout(timer)
    if (!storageError.value) timer = setTimeout(() => void persist(), 80)
  }
  function replace() {
    full = true
    schedule()
  }
  async function persist(): Promise<boolean> {
    clearTimeout(timer)
    if (task) {
      const ok = await task
      return ok && dirty.value ? persist() : ok
    }
    task = (async () => {
      saving.value = true
      try {
        while (dirty.value) {
          const batch: Changes = pending,
            entire = full
          pending = emptyChanges()
          full = false
          dirty.value = false
          try {
            await saveState(state, entire ? undefined : batch)
            storageError.value = ''
          } catch (e) {
            mergeChanges(pending, batch)
            full ||= entire
            dirty.value = true
            storageError.value = e instanceof Error ? e.message : String(e)
            return false
          }
        }
        return true
      } finally {
        saving.value = false
      }
    })()
    const ok = await task
    task = undefined
    return ok
  }
  function dispose() {
    clearTimeout(timer)
  }
  return { dirty, saving, storageError, mark, message, replace, persist, dispose }
}
