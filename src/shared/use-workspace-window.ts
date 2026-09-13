import { onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useCodexStore } from '../features/codex/store'
import { isDesktop } from './desktop'
import { closeWorkspace } from './window-close'

export function useWorkspaceWindow(beforeClose: () => void, afterLoad?: () => Promise<void>) {
  const codex = useCodexStore()
  const closeError = ref('')
  let unlisten: (() => void) | undefined
  let disposed = false
  async function attemptClose(discard = false) {
    if (codex.closing) return
    beforeClose()
    codex.closing = true
    closeError.value = ''
    try {
      await closeWorkspace(
        {
          save: () => (codex.initialized ? codex.flush() : Promise.resolve(true)),
          disconnect: () => invoke('codex_disconnect'),
          destroy: () => getCurrentWindow().destroy(),
        },
        discard,
      )
    } catch (error) {
      closeError.value = error instanceof Error ? error.message : String(error)
    } finally {
      codex.closing = false
    }
  }
  onMounted(async () => {
    if (isDesktop()) {
      unlisten = await getCurrentWindow().onCloseRequested((event) => {
        event.preventDefault()
        void attemptClose()
      })
      if (disposed) {
        unlisten()
        return
      }
    }
    await codex.initializeWorkspace()
    if (!disposed && !codex.closing && codex.initialized) await afterLoad?.()
  })
  onUnmounted(() => {
    disposed = true
    unlisten?.()
  })
  return { codex, closeError, attemptClose }
}
