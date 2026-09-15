import assert from 'node:assert/strict'
import { readFile, readdir, access } from 'node:fs/promises'

const html = await readFile('dist-web/index.html', 'utf8')
if (!process.env.PARLEY_WEB_BASE || process.env.PARLEY_WEB_BASE === './') {
  assert(!/(?:src|href)="\//.test(html), 'Web assets must use relative paths for GitHub Pages')
}
await access('dist-web/app-icon.svg')
const files = await readdir('dist-web/assets')
const javascript = (
  await Promise.all(
    files
      .filter((name) => name.endsWith('.js'))
      .map((name) => readFile(`dist-web/assets/${name}`, 'utf8')),
  )
).join('\n')
assert(javascript.includes('parley-web'), 'Expected the functional Web entry')
assert(!javascript.includes('__TAURI_INTERNALS__'), 'Web bundle must not contain desktop IPC')
console.log('Web entry, relative assets and API-only bundle verified')
