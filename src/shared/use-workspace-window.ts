import { onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useWorkspaceStore } from '../features/workspace/store'
import { useChatStore } from '../features/chat/store'
import { isDesktop } from './desktop'
import { closeWorkspace } from './window-close'
import { useVocabularyStore } from '../features/vocabulary/store'
import { settleWordOperations } from './word-operations'

export function useWorkspaceWindow(beforeClose: () => void, afterLoad?: () => Promise<void>) {
  const workspace = useWorkspaceStore()
  const codex = useChatStore()
  const vocabulary = useVocabularyStore()
  const closeError = ref('')
  let unlisten: (() => void) | undefined
  let disposed = false
  async function attemptClose(discard = false) {
    if (workspace.closing) return
    beforeClose()
    workspace.closing = true
    closeError.value = ''
    try {
      await closeWorkspace(
        {
          save: async () => {
            const wordsSaved = await vocabulary.flush()
            await settleWordOperations()
            const chatSaved = workspace.initialized ? await workspace.flush() : true
            return wordsSaved && chatSaved
          },
          disconnect: () => invoke('backend_disconnect'),
          destroy: () => getCurrentWindow().destroy(),
        },
        discard,
      )
    } catch (error) {
      closeError.value = error instanceof Error ? error.message : String(error)
    } finally {
      workspace.closing = false
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
    await workspace.initialize()
    if (workspace.initialized) await vocabulary.initialize()
    if (disposed || workspace.closing) return
    await codex.initializeWorkspace()
    if (!disposed && !workspace.closing && workspace.initialized) await afterLoad?.()
  })
  onUnmounted(() => {
    disposed = true
    unlisten?.()
  })
  return { codex, workspace, closeError, attemptClose }
}
