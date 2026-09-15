import type { Provider } from '../backends/types'
import type { VocabularySource } from './types'

const providers: Record<Provider, string> = {
  openai: 'OpenAI',
  anthropic: 'Claude',
  google: 'Gemini',
  deepseek: 'DeepSeek',
  qwen: '通义千问',
  kimi: 'Kimi',
  zai: 'GLM / Z.AI',
  custom: '自定义 API',
}

export function sourceLabel(source: VocabularySource): string {
  if (source.sourceKind === 'manual') return '手动添加'
  if (source.sourceKind === 'import') return '导入材料'
  const backend = source.backend
  const service = backend
    ? backend.kind === 'codex'
      ? 'Codex'
      : backend.kind === 'claude_code'
        ? 'Claude Code'
        : providers[backend.provider]
    : source.sourceKind === 'terminal'
      ? source.threadId?.startsWith('claude-code:')
        ? 'Claude Code'
        : 'Codex'
      : '后端未知'
  const location =
    source.sourceKind === 'terminal'
      ? '终端'
      : source.sourceKind === 'tutor'
        ? '学习助手'
        : '主对话'
  return [service, location, backend?.model].filter(Boolean).join(' · ')
}
