import { afterEach, describe, expect, it, vi } from 'vitest'
import { closeWorkspace } from './window-close'

afterEach(() => vi.useRealTimers())
const actions = () => ({
  save: vi.fn(async () => true),
  disconnect: vi.fn(async () => undefined),
  destroy: vi.fn(async () => undefined),
})
describe('closing the workspace', () => {
  it('does not close or disconnect if saving fails', async () => {
    const a = actions()
    a.save.mockResolvedValue(false)
    await expect(closeWorkspace(a)).rejects.toThrow('草稿未能保存')
    expect(a.disconnect).not.toHaveBeenCalled()
    expect(a.destroy).not.toHaveBeenCalled()
  })
  it('returns control when saving hangs and permits a later retry', async () => {
    vi.useFakeTimers()
    const a = actions()
    a.save.mockImplementationOnce(() => new Promise(() => {}))
    const failed = expect(closeWorkspace(a)).rejects.toThrow('保存耗时过长')
    await vi.advanceTimersByTimeAsync(5000)
    await failed
    expect(a.destroy).not.toHaveBeenCalled()
    await closeWorkspace(a)
    expect(a.destroy).toHaveBeenCalledOnce()
  })
  it('returns control when disconnect hangs, and only discards on explicit request', async () => {
    vi.useFakeTimers()
    const a = actions()
    a.disconnect.mockImplementation(() => new Promise(() => {}))
    const failed = expect(closeWorkspace(a)).rejects.toThrow('模型后端未能及时退出')
    await vi.advanceTimersByTimeAsync(12000)
    await failed
    expect(a.destroy).not.toHaveBeenCalled()
    a.save.mockClear()
    const forced = closeWorkspace(a, true)
    await vi.advanceTimersByTimeAsync(12000)
    await forced
    expect(a.save).not.toHaveBeenCalled()
    expect(a.destroy).toHaveBeenCalledOnce()
  })
  it('reports a destroy failure so another close can be attempted', async () => {
    const a = actions()
    a.destroy.mockRejectedValueOnce(new Error('window failure'))
    await expect(closeWorkspace(a)).rejects.toThrow('window failure')
    await closeWorkspace(a)
    expect(a.destroy).toHaveBeenCalledTimes(2)
  })
})
