export type BackendKind =
  | 'codex'
  | 'claude_code'
  | 'openai_responses'
  | 'anthropic_messages'
  | 'gemini_interactions'
  | 'openai_compatible'
export type Provider =
  'openai' | 'anthropic' | 'google' | 'deepseek' | 'qwen' | 'kimi' | 'zai' | 'custom'
export interface ProfileConfig {
  name: string
  kind: BackendKind
  provider: Provider
  endpoint: string
  binaryPath: string
  enabled: boolean
}
export interface BackendProfile {
  id: string
  revision: number
  config: ProfileConfig
}
export interface CredentialStatus {
  configured: boolean
  persistence: 'session' | 'system' | 'missing'
}
export interface BackendRuntime {
  credential: CredentialStatus
  models: string[]
  checking: boolean
  error: string
}
export interface TurnEvent {
  profileId: string
  profileRevision: number
  conversationId: string
  pane: 'main' | 'tutor'
  turnId: string
  requestId: string
  messageId: string
  sequence: number
  status: 'streaming' | 'complete' | 'failed' | 'interrupted'
  text: string
  usage: Record<string, unknown> | null
  error: string | null
  notice: string | null
}
export const supportsManagedTurns = (kind: BackendKind) =>
  kind === 'openai_responses' ||
  kind === 'openai_compatible' ||
  kind === 'anthropic_messages' ||
  kind === 'gemini_interactions' ||
  kind === 'claude_code'

export const presets: { label: string; config: ProfileConfig; available: boolean }[] = [
  {
    label: 'OpenAI API',
    config: {
      name: 'OpenAI API',
      kind: 'openai_responses',
      provider: 'openai',
      endpoint: 'https://api.openai.com/v1',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: 'DeepSeek',
    config: {
      name: 'DeepSeek',
      kind: 'openai_compatible',
      provider: 'deepseek',
      endpoint: 'https://api.deepseek.com/v1',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: '通义千问（自行选择区域地址）',
    config: {
      name: '通义千问',
      kind: 'openai_compatible',
      provider: 'qwen',
      endpoint: '',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: 'Kimi（国际服务）',
    config: {
      name: 'Kimi',
      kind: 'openai_compatible',
      provider: 'kimi',
      endpoint: 'https://api.moonshot.ai/v1',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: 'GLM / Z.AI（国际服务）',
    config: {
      name: 'GLM / Z.AI',
      kind: 'openai_compatible',
      provider: 'zai',
      endpoint: 'https://api.z.ai/api/paas/v4',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: '自定义 Chat Completions 兼容服务',
    config: {
      name: '自定义 API',
      kind: 'openai_compatible',
      provider: 'custom',
      endpoint: '',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: 'Claude API',
    config: {
      name: 'Claude API',
      kind: 'anthropic_messages',
      provider: 'anthropic',
      endpoint: 'https://api.anthropic.com/v1',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: 'Gemini API',
    config: {
      name: 'Gemini API',
      kind: 'gemini_interactions',
      provider: 'google',
      endpoint: 'https://generativelanguage.googleapis.com/v1beta',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
  {
    label: 'Claude Code（API Key）',
    config: {
      name: 'Claude Code',
      kind: 'claude_code',
      provider: 'anthropic',
      endpoint: '',
      binaryPath: '',
      enabled: true,
    },
    available: true,
  },
]
