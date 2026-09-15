<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from '../../shared/desktop'
import { useChatStore } from '../chat/store'
import TerminalContext from './TerminalContext.vue'
interface Session {
  id: string
  backend: 'codex' | 'claude-code'
  binary: string
  cwd: string
  startedAt: number
  status: 'running' | 'ended' | 'unknown'
}
const chat = useChatStore()
const sessions = ref<Session[]>([]),
  selected = ref(''),
  busy = ref(false),
  error = ref('')
const current = computed(() => sessions.value.find((s) => s.id === selected.value))
const blocked = computed(() => busy.value || chat.lanes.main.busy || chat.lanes.tutor.busy)
const labels = { running: '运行中', ended: '已结束 · 历史缓存', unknown: '状态未知 · 旧记录' }
let timer: ReturnType<typeof setTimeout> | undefined
let disposed = false
async function refresh() {
  clearTimeout(timer)
  if (!isDesktop() || disposed) return
  try {
    const result = await invoke<{ sessions: Session[]; selectedId: string | null }>(
      'companion_list',
    )
    if (disposed) return
    sessions.value = result.sessions
    selected.value = result.selectedId ?? ''
  } catch (e) {
    if (!disposed) error.value = String(e)
  } finally {
    if (!disposed) timer = setTimeout(() => void refresh(), 5000)
  }
}
async function select(id: string) {
  if (blocked.value) return
  busy.value = true
  error.value = ''
  try {
    // Clear the old context before asynchronous connection/switching.
    chat.terminalContext = ''
    const session = sessions.value.find((s) => s.id === id)
    if (session?.backend === 'codex') {
      if (chat.codexPath !== session.binary || !chat.connected) {
        chat.codexPath = session.binary
        await chat.connect()
        if (!chat.connected) throw new Error(chat.error || 'Codex 连接失败。')
      }
    }
    await invoke('companion_select', { id: id || null })
    selected.value = id
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
    await refresh()
  }
}
async function remove(session: Session) {
  if (blocked.value || !confirm(`清理 ${session.cwd} 的这次伴随缓存？学习词句和辅导记录会保留。`))
    return
  busy.value = true
  error.value = ''
  try {
    await invoke('companion_remove', { id: session.id })
    await refresh()
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
onMounted(() => void refresh())
onUnmounted(() => {
  disposed = true
  clearTimeout(timer)
})
</script>
<template>
  <section v-if="isDesktop()" class="settings-section companion-sessions" aria-label="伴随会话管理">
    <details>
      <summary>伴随会话 · {{ current ? labels[current.status] : '未关联' }}</summary>
      <p class="settings-help">
        选择已启动的终端；已结束的记录可查看缓存。切换不会启动第二个原生 CLI。
      </p>
      <label
        >关联终端
        <select
          :value="selected"
          :title="current?.cwd"
          :disabled="blocked"
          @change="select(($event.target as HTMLSelectElement).value)"
        >
          <option value="">不关联终端</option>
          <option v-for="session in sessions" :key="session.id" :value="session.id">
            {{ labels[session.status] }} ·
            {{ session.backend === 'codex' ? 'Codex' : 'Claude Code' }} · {{ session.cwd }} ·
            {{ new Date(session.startedAt * 1000).toLocaleString() }} · {{ session.id.slice(0, 8) }}
          </option>
        </select>
      </label>
      <div
        v-for="session in sessions.filter((s) => s.status !== 'running')"
        :key="session.id"
        class="companion-cache-row"
      >
        <span class="settings-help"
          >{{ labels[session.status] }} · {{ session.cwd }} · {{ session.id.slice(0, 8) }}</span
        >
        <button
          class="text-button"
          :disabled="blocked || session.id === selected"
          @click="remove(session)"
        >
          清理缓存
        </button>
      </div>
      <button class="text-button" :disabled="busy" @click="refresh">刷新状态</button>
    </details>
    <p v-if="current && current.status !== 'running'" class="settings-help" role="status">
      {{ labels[current.status] }}。{{
        current.status === 'ended'
          ? '当前展示已保存的内容；此终端已经退出。'
          : '无法确认终端是否仍在运行，已收到的内容仍可查看。'
      }}
    </p>
    <TerminalContext
      v-if="current && !busy"
      :key="current.id"
      :started-at="current.startedAt"
      :backend="current.backend"
    />
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
  </section>
</template>
<style scoped>
.companion-sessions {
  min-width: 0;
  max-width: 100%;
}
label {
  display: grid;
  overflow: hidden;
  gap: 0.5rem;
  min-width: 0;
}
select {
  width: 100%;
  min-width: 0;
  max-width: 100%;
}
.companion-cache-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-block: 0.5rem;
}
.companion-cache-row span {
  min-width: 0;
  overflow-wrap: anywhere;
}
.companion-cache-row button {
  flex-shrink: 0;
}
</style>
