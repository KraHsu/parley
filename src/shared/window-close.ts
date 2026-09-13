interface CloseActions {
  save: () => Promise<boolean>
  disconnect: () => Promise<unknown>
  destroy: () => Promise<void>
}

async function within<T>(action: () => Promise<T>, milliseconds: number, message: string) {
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    return await Promise.race([
      action(),
      new Promise<never>((_, reject) => {
        timer = setTimeout(() => reject(new Error(message)), milliseconds)
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

export async function closeWorkspace(actions: CloseActions, discard = false) {
  if (!discard) {
    const saved = await within(actions.save, 5000, '保存耗时过长，请重试关闭。')
    if (!saved) throw new Error('草稿未能保存，请重试，或关闭并放弃未保存修改。')
  }
  try {
    await within(actions.disconnect, 12000, 'Codex 未能及时退出，请重试或强制关闭。')
  } catch (error) {
    if (!discard) throw error
  }
  await within(actions.destroy, 5000, '窗口关闭失败，请重试。')
}
