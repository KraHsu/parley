import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync, statSync } from 'node:fs'
import { join } from 'node:path'

assert.equal(process.platform, 'win32', 'Windows packaging requires a Windows host')
assert.equal(process.arch, 'x64', 'This packaging command targets Windows x64')
const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
const metadata = JSON.parse(
  execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1', '--locked'], {
    encoding: 'utf8',
  }),
)
const release = join(metadata.target_directory, 'release')
const name = `${config.productName}_${config.version}_x64-setup.exe`
const directory = join(release, 'bundle/nsis')
for (const file of [
  join(directory, name),
  ...['parley.exe', 'parley-cli.exe'].map((f) => join(release, f)),
]) {
  assert(statSync(file).size > 0, `Missing Windows binary: ${file}`)
  const bytes = readFileSync(file)
  assert.equal(bytes.toString('ascii', 0, 2), 'MZ', `Invalid executable: ${file}`)
  const pe = bytes.readUInt32LE(0x3c)
  assert.equal(bytes.readUInt32LE(pe), 0x4550, `Invalid PE header: ${file}`)
  // NSIS uses an x86 bootstrapper even when its payload is x64.
  if (!file.endsWith('-setup.exe')) assert.equal(bytes.readUInt16LE(pe + 4), 0x8664)
}
const sha = createHash('sha256')
  .update(readFileSync(join(directory, name)))
  .digest('hex')
writeFileSync(join(directory, 'SHA256SUMS-windows-x64'), `${sha}  ${name}\n`)
// Tauri patches the main executable's bundle marker for NSIS, then restores the
// build-directory executable. Compare the installed bytes against exactly that
// bundler transformation, not against the restored raw executable.
// https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle.rs
const payload = {}
for (const binary of ['parley.exe', 'parley-cli.exe']) {
  const bytes = readFileSync(join(release, binary))
  if (binary === 'parley.exe') {
    const marker = Buffer.from('__TAURI_BUNDLE_TYPE_VAR_UNK')
    const offset = bytes.indexOf(marker)
    assert(offset >= 0, 'Tauri bundle marker missing; recheck the bundler transformation')
    Buffer.from('__TAURI_BUNDLE_TYPE_VAR_NSS').copy(bytes, offset)
  }
  payload[binary] = createHash('sha256').update(bytes).digest('hex')
}
mkdirSync('artifacts/windows-install', { recursive: true })
writeFileSync('artifacts/windows-install/payload-hashes.json', JSON.stringify(payload, null, 2))
console.log(`Windows x64 package ready: ${join(directory, name)}\nSHA256: ${sha}`)
