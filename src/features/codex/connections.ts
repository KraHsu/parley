import { reactive, type UnwrapRef } from 'vue'
import { defineStore } from 'pinia'
import { createCodexConnection, useCodexConnectionStore, type ServerEvent } from './connection'

type Connection = UnwrapRef<ReturnType<typeof createCodexConnection>>
export const useCodexConnectionsStore = defineStore('codex-connections', () => {
  const primary = useCodexConnectionStore()
  const connections = reactive<Record<string, Connection>>({})
  const listeners = new Set<(event: ServerEvent) => void>()
  function get(id: string): Connection {
    if (id === 'codex-default') return primary
    if (!connections[id]) {
      const connection = reactive(createCodexConnection(id))
      connection.onEvent((event) => {
        for (const listener of listeners) listener(event)
      })
      connections[id] = connection
    }
    return connections[id]!
  }
  function onEvent(listener: (event: ServerEvent) => void) {
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }
  return { get, onEvent }
})
