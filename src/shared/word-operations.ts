import { invoke } from '@tauri-apps/api/core'
const pending = new Set<Promise<unknown>>()
// Domain operations remain tracked even when their component is hidden or unmounted.
export function wordInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const task = invoke<T>(command, args)
  pending.add(task)
  void task.then(
    () => pending.delete(task),
    () => pending.delete(task),
  )
  return task
}
export async function settleWordOperations() {
  while (pending.size) await Promise.allSettled([...pending])
}
