import http from 'node:http'
import { readFile } from 'node:fs/promises'
import { resolve, extname } from 'node:path'
const root = resolve('dist-web')
function api(req, res) {
  res.setHeader('Access-Control-Allow-Origin', 'http://127.0.0.1:4178')
  res.setHeader('Access-Control-Allow-Headers', 'authorization,content-type')
  if (req.method === 'OPTIONS') {
    res.end()
    return
  }
  if (req.url === '/v1/models') {
    res.setHeader('Content-Type', 'application/json')
    res.end(JSON.stringify({ data: [{ id: 'fixture' }] }))
    return
  }
  if (
    req.url !== '/v1/responses' ||
    req.method !== 'POST' ||
    req.headers.authorization !== 'Bearer fixture-key'
  ) {
    res.writeHead(401)
    res.end()
    return
  }
  let body = ''
  req.on('data', (chunk) => {
    body += chunk
  })
  req.on('end', () => {
    const input = JSON.parse(body)
    if (input.model === 'missing-model') {
      res.writeHead(400, { 'Content-Type': 'application/json', 'x-request-id': 'req_fixture' })
      res.end(
        JSON.stringify({
          error: { code: 'model_not_found', message: 'request may contain private text' },
        }),
      )
      return
    }
    const question = input.input.at(-1).content
    const output =
      input.instructions.includes('Task: translate') && input.input.length > 1
        ? '这是刚才那句话的翻译。'
        : 'We can break the ice with a friendly hello.'
    res.writeHead(200, { 'Content-Type': 'text/event-stream', 'Cache-Control': 'no-cache' })
    res.write(`data: ${JSON.stringify({ type: 'response.output_text.delta', delta: output })}\n\n`)
    const timer = setTimeout(
      () => {
        res.end(
          `data: ${JSON.stringify({ type: 'response.completed', response: { status: 'completed', output: [{ type: 'message', role: 'assistant', content: [{ type: 'output_text', text: output }] }], usage: { input_tokens: 3, output_tokens: 10 } } })}\n\n`,
        )
      },
      String(question).includes('slow') ? 1500 : 20,
    )
    res.on('close', () => clearTimeout(timer))
  })
}
const app = http.createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://localhost').pathname)
  const relative = path.replace(/^\/parley\/?/, '')
  const file = resolve(root, relative || 'index.html')
  if (!path.startsWith('/parley/') || !file.startsWith(root + '/')) {
    res.writeHead(404)
    res.end()
    return
  }
  try {
    res.setHeader(
      'Content-Type',
      {
        '.html': 'text/html',
        '.js': 'text/javascript',
        '.css': 'text/css',
        '.svg': 'image/svg+xml',
      }[extname(file)] ?? 'application/octet-stream',
    )
    res.end(await readFile(file))
  } catch {
    res.writeHead(404, { 'Content-Type': 'text/html' })
    res.end('<!doctype html><title>Test setup</title>')
  }
})
const fixture = http.createServer(api)
app.listen(4178, '127.0.0.1')
fixture.listen(4179, '127.0.0.1')
function shutdown() {
  app.close()
  fixture.close()
  app.closeAllConnections()
  fixture.closeAllConnections()
}
process.on('SIGTERM', shutdown)
process.on('SIGINT', shutdown)
