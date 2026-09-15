export type ApiErrorKind = 'credentials' | 'quota' | 'request' | 'service'
import catalog from '../../fixtures/api-errors.json'
const reasons = catalog as unknown as Record<string, [ApiErrorKind, string]>
export class ApiError extends Error {
  constructor(
    readonly kind: ApiErrorKind,
    readonly status: number,
    reason: string,
    readonly code?: string,
    readonly requestId?: string,
  ) {
    super(
      `${reason}（HTTP ${status}${code ? ` · ${code}` : ''}${requestId ? ` · 请求 ${requestId}` : ''}）`,
    )
  }
}
export async function apiError(response: Response, secrets: string[] = []): Promise<ApiError> {
  let raw = '',
    bytes = 0
  const reader = response.body?.getReader(),
    decoder = new TextDecoder()
  try {
    if (reader)
      while (true) {
        const { value, done } = await reader.read()
        if (done) break
        bytes += value.byteLength
        if (bytes > 16384) break
        raw += decoder.decode(value, { stream: true })
      }
  } catch {
    /* HTTP status remains usable even when the error body fails. */
  } finally {
    await reader?.cancel().catch(() => {})
    reader?.releaseLock()
  }
  let code: string | undefined
  try {
    const json = JSON.parse(raw),
      e = json.error ?? json
    code = [e.code, e.type, e.status].find(
      (v) => typeof v === 'string' && Object.hasOwn(reasons, v),
    )
    if ((!code || code === 'invalid_request_error') && typeof e.message === 'string') {
      const message = e.message.toLowerCase()
      if (/context.{0,20}(length|window)|maximum context/.test(message))
        code = 'context_length_exceeded'
      else if (/unsupported parameter|parameter.{0,30}not supported/.test(message))
        code = 'unsupported_parameter'
      else if (/model.{0,40}(not found|does not exist)/.test(message)) code = 'model_not_found'
    }
  } catch {
    /* HTML/proxy bodies are never displayed. */
  }
  const fallback: [ApiErrorKind, string] =
    response.status === 401 || response.status === 403
      ? ['credentials', '认证或访问权限不足，请检查 API Key。']
      : response.status === 429
        ? ['quota', '服务限制了请求，请检查额度并稍后重试。']
        : response.status >= 500
          ? ['service', '服务暂时不可用，请稍后重试。']
          : ['request', '请求未被接受，请检查服务地址、模型 ID 与 API 协议。']
  const [kind, reason] = (code && reasons[code]) || fallback
  const id = response.headers.get('x-request-id') ?? response.headers.get('request-id')
  const requestId =
    id && /^[a-zA-Z0-9_-]{1,128}$/.test(id) && !secrets.some((s) => s && id.includes(s))
      ? id
      : undefined
  return new ApiError(kind, response.status, reason, code, requestId)
}
