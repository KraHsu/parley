import { createPinia } from 'pinia'
import { createApp } from 'vue'
import App from './App.vue'
import TutorApp from './TutorApp.vue'
import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from './shared/desktop'
import './styles/main.css'
import './styles/vocabulary.css'

async function start() {
  const options = isDesktop()
    ? await invoke<{
        tutorOnly: boolean
        codexPath: string | null
        terminalCwd: string | null
        terminalStartedAt: number
        terminalBackend: 'codex' | 'claude-code'
      }>('get_launch_options')
    : {
        tutorOnly: new URLSearchParams(location.search).get('mode') === 'tutor',
        codexPath: null,
        terminalCwd: null,
        terminalStartedAt: 0,
        terminalBackend: 'codex' as const,
      }
  createApp(options.tutorOnly ? TutorApp : App, {
    initialCodexPath: options.codexPath,
    terminalCwd: options.terminalCwd,
    terminalStartedAt: options.terminalStartedAt,
    terminalBackend: options.terminalBackend,
  })
    .use(createPinia())
    .mount('#app')
}
void start().catch((error) => {
  const root = document.querySelector('#app')
  if (root) root.textContent = `无法初始化 Parley：${String(error)}。请关闭后重新启动。`
})
