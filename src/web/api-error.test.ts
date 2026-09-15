import { describe, expect, it } from 'vitest'
import { apiError } from './api-error'
describe('API diagnostics', () => {
  it('distinguishes request failures and preserves a bounded request ID without revealing provider text', async () => {
    for (const [code, text] of [
      ['model_not_found', '模型不存在'],
      ['context_length_exceeded', '上下文'],
      ['unsupported_parameter', '参数'],
      ['insufficient_quota', '额度不足'],
    ]) {
      const response = new Response(
        JSON.stringify({ error: { code, message: 'private prompt and sk-secret' } }),
        { status: 400, headers: { 'x-request-id': 'req_123' } },
      )
      const error = await apiError(response)
      expect(error.message).toContain(text)
      expect(error.requestId).toBe('req_123')
      expect(error.message).not.toContain('private')
      expect(error.message).not.toContain('sk-secret')
    }
  })
  it('handles HTML and oversized bodies and refuses credentials as a request ID', async () => {
    const response = new Response('x'.repeat(20000), {
      status: 502,
      headers: { 'x-request-id': 'sk-secret' },
    })
    const error = await apiError(response, ['sk-secret'])
    expect(error.kind).toBe('service')
    expect(error.requestId).toBeUndefined()
    expect(error.message).not.toContain('sk-secret')
  })
})
