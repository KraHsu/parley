import { afterEach, describe, expect, it, vi } from 'vitest'
import fixtures from '../../src-tauri/src/backends/fixtures/compatible.json'
import { buildRequest, endpoint, generate, listModels, Output, SseDecoder } from './api'
import { apiPresets, newConversation, type ApiProfile } from './types'
const profile = (kind: ApiProfile['kind'] = 'openai_responses'): ApiProfile => ({
  id: 'test',
  name: 'Test',
  kind,
  provider:
    kind === 'anthropic_messages'
      ? 'anthropic'
      : kind === 'gemini_interactions'
        ? 'google'
        : 'openai',
  endpoint: 'https://api.example.test/v1',
})
const end = {
  type: 'response.completed',
  response: {
    status: 'completed',
    output: [
      { type: 'reasoning', encrypted_content: 'opaque' },
      { type: 'message', content: [{ type: 'output_text', text: 'Bonjour 🌍' }] },
    ],
    usage: { output_tokens: 4 },
  },
}
const events = [{ type: 'response.output_text.delta', delta: 'Bonjour 🌍' }, end]
afterEach(() => vi.unstubAllGlobals())
describe('web API protocols', () => {
  it('excludes local CLI backends and unsafe endpoints', () => {
    expect(apiPresets).toHaveLength(8)
    expect(apiPresets.some((p) => ['codex', 'claude_code'].includes(p.config.kind))).toBe(false)
    for (const url of [
      'http://example.com',
      'https://user:pass@example.com',
      'https://example.com?api_key=secret',
      'javascript:alert(1)',
    ])
      expect(() => endpoint({ ...profile(), endpoint: url })).toThrow()
    expect(endpoint({ ...profile(), endpoint: 'http://localhost:1234/v1/' })).toBe(
      'http://localhost:1234/v1',
    )
  })
  it('decodes every UTF-8 byte and split CRLF without losing event boundaries', () => {
    const bytes = new TextEncoder().encode(
      events.map((e) => `: keepalive\r\ndata: ${JSON.stringify(e)}\r\n\r\n`).join(''),
    )
    const decoder = new TextDecoder('utf-8', { fatal: true }),
      sse = new SseDecoder(),
      output = new Output('openai_responses')
    for (const byte of bytes)
      for (const data of sse.feed(decoder.decode(new Uint8Array([byte]), { stream: true })))
        output.accept(data)
    expect(output.text).toBe('Bonjour 🌍')
    expect(output.complete).toBe(true)
    expect(output.continuation).toEqual(end.response.output)
    const multiline = new SseDecoder()
    expect(multiline.feed('data: {\ndata: "a": 1}\n\n')).toEqual(['{\n"a": 1}'])
  })
  for (const fixture of fixtures)
    it(`reuses desktop ${fixture.provider} compatibility events`, () => {
      const output = new Output('openai_compatible')
      for (const event of fixture.events) output.accept(JSON.stringify(event))
      output.accept('[DONE]')
      expect(output.complete).toBe(true)
      expect(output.text).toBe('Bonjour 🌍')
      expect(output.text).not.toContain('思考')
      const p = {
        ...profile('openai_compatible'),
        provider: fixture.provider as ApiProfile['provider'],
      }
      const { body } = buildRequest(
        p,
        newConversation('main', p.id, fixture.model),
        'Hello',
        'conversation',
      )
      expect(body.stream_options?.include_usage ?? false).toBe(fixture.stream_options)
      expect(body.temperature).toBeUndefined()
    })
  it('retains native Claude blocks and signatures without displaying thinking', () => {
    const output = new Output('anthropic_messages')
    for (const e of [
      {
        type: 'message_start',
        message: { role: 'assistant', content: [], usage: { input_tokens: 3 } },
      },
      {
        type: 'content_block_start',
        index: 0,
        content_block: { type: 'thinking', thinking: '', signature: '' },
      },
      {
        type: 'content_block_delta',
        index: 0,
        delta: { type: 'thinking_delta', thinking: 'private' },
      },
      {
        type: 'content_block_delta',
        index: 0,
        delta: { type: 'signature_delta', signature: 'signed' },
      },
      { type: 'content_block_stop', index: 0 },
      { type: 'content_block_start', index: 1, content_block: { type: 'text', text: '' } },
      { type: 'content_block_delta', index: 1, delta: { type: 'text_delta', text: 'Bonjour' } },
      { type: 'content_block_stop', index: 1 },
      { type: 'message_delta', delta: { stop_reason: 'end_turn' }, usage: { output_tokens: 2 } },
      { type: 'message_stop' },
    ])
      output.accept(JSON.stringify(e))
    expect(output.text).toBe('Bonjour')
    expect(output.complete).toBe(true)
    expect(output.continuation).toEqual([
      { type: 'thinking', thinking: 'private', signature: 'signed' },
      { type: 'text', text: 'Bonjour' },
    ])
    expect(output.usage).toEqual({ input_tokens: 3, output_tokens: 2 })
  })
  it('keeps Gemini steps for stateless continuation', () => {
    const output = new Output('gemini_interactions')
    for (const e of [
      { event_type: 'interaction.created', interaction: { status: 'in_progress' } },
      { event_type: 'step.start', index: 0, step: { type: 'model_output', content: [] } },
      { event_type: 'step.delta', index: 0, delta: { type: 'text', text: 'Bonjour' } },
      { event_type: 'step.stop', index: 0 },
      {
        event_type: 'interaction.completed',
        interaction: { status: 'completed', usage: { total_tokens: 5 } },
      },
    ])
      output.accept(JSON.stringify(e))
    const c = newConversation('main', 'test', 'test-model')
    c.messages = [
      { id: 'u', role: 'user', text: 'Hi', status: 'complete' },
      {
        id: 'a',
        role: 'assistant',
        text: output.text,
        status: 'complete',
        continuation: output.continuation,
      },
    ]
    expect(
      buildRequest(profile('gemini_interactions'), c, 'Again', 'conversation').body.input,
    ).toEqual([
      { type: 'user_input', content: [{ type: 'text', text: 'Hi' }] },
      { type: 'model_output', content: [{ type: 'text', text: 'Bonjour' }] },
      { type: 'user_input', content: [{ type: 'text', text: 'Again' }] },
    ])
  })
  it('rejects tools, unmatched blocks, output truncation and missing success markers', () => {
    const output = new Output('openai_compatible')
    output.accept(JSON.stringify({ choices: [{ delta: { content: 'partial' } }] }))
    expect(() => output.accept('[DONE]')).toThrow()
    expect(() =>
      new Output('anthropic_messages').accept(
        JSON.stringify({
          type: 'content_block_delta',
          index: 0,
          delta: { type: 'text_delta', text: 'x' },
        }),
      ),
    ).toThrow()
    expect(() =>
      new Output('openai_responses').accept(
        JSON.stringify({ type: 'response.output_item.added', item: { type: 'function_call' } }),
      ),
    ).toThrow()
    expect(() =>
      new Output('openai_compatible').accept(
        JSON.stringify({ choices: [{ finish_reason: 'length', delta: { content: 'part' } }] }),
      ),
    ).toThrow()
  })
  it('budgets complete pairs, excludes failed history and preserves current input', () => {
    const c = newConversation('tutor', 'test', 'model')
    for (let i = 0; i < 20; i++)
      c.messages.push(
        { id: `u${i}`, role: 'user', text: `old-${i}` + 'a'.repeat(10000), status: 'complete' },
        { id: `a${i}`, role: 'assistant', text: `reply-${i}`, status: 'complete' },
      )
    c.messages.push(
      { id: 'uf', role: 'user', text: 'failed-input', status: 'complete' },
      { id: 'af', role: 'assistant', text: 'failed-output', status: 'failed' },
    )
    const { body, clipped } = buildRequest(profile(), c, 'current', 'translate')
    expect(clipped).toBe(true)
    expect(JSON.stringify(body)).not.toContain('failed-input')
    expect(JSON.stringify(body)).not.toContain('old-0')
    expect(body.input.at(-1)).toEqual({ role: 'user', content: 'current' })
    expect(() => buildRequest(profile(), c, 'a'.repeat(100000), 'translate')).toThrow()
  })
  it('sends credentials only in headers, streams and cancels the reader at the terminal event', async () => {
    const cancel = vi.fn()
    const fetch = vi.fn(
      async () =>
        new Response(
          new ReadableStream({
            start(controller) {
              controller.enqueue(
                new TextEncoder().encode(
                  events.map((e) => `data: ${JSON.stringify(e)}\n\n`).join(''),
                ),
              )
            },
            cancel,
          }),
          { headers: { 'Content-Type': 'text/event-stream' } },
        ),
    )
    vi.stubGlobal('fetch', fetch)
    const changed = vi.fn()
    await generate(
      profile(),
      newConversation('main', 'test', 'model'),
      'private-key',
      'Hi',
      'conversation',
      new AbortController().signal,
      changed,
    )
    const options = (fetch.mock.calls as unknown as [string, RequestInit][])[0]![1]
    expect(options.redirect).toBe('error')
    expect(options.credentials).toBe('omit')
    expect(options.body).not.toContain('private-key')
    expect(new Headers(options.headers).get('authorization')).toBe('Bearer private-key')
    expect(changed.mock.lastCall?.[0].text).toBe('Bonjour 🌍')
    expect(cancel).toHaveBeenCalledOnce()
  })
  it('surfaces CORS errors and refuses non-streaming success or early EOF', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('fetch failed')))
    await expect(listModels(profile(), 'key')).rejects.toThrow('CORS')
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('{}', { headers: { 'Content-Type': 'application/json' } })),
    )
    await expect(
      generate(
        profile(),
        newConversation('main'),
        'key',
        'Hi',
        'conversation',
        new AbortController().signal,
        () => {},
      ),
    ).rejects.toThrow('SSE')
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response('data: {"type":"response.output_text.delta","delta":"partial"}\n\n', {
            headers: { 'Content-Type': 'text/event-stream' },
          }),
      ),
    )
    await expect(
      generate(
        profile(),
        newConversation('main'),
        'key',
        'Hi',
        'conversation',
        new AbortController().signal,
        () => {},
      ),
    ).rejects.toThrow('提前结束')
  })
})
