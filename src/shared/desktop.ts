import { invoke, isTauri } from '@tauri-apps/api/core'
import type { RuntimeInfo } from './types'

export function isDesktop(): boolean {
  return isTauri()
}

export async function getRuntimeInfo(): Promise<RuntimeInfo> {
  if (!isDesktop()) {
    throw new Error('当前为浏览器预览。请运行 npm run desktop:dev 检查桌面连接。')
  }

  return invoke<RuntimeInfo>('get_runtime_info')
}
