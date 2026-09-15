import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const run = (command, args) => execFileSync(command, args, { encoding: 'utf8' }).trim()
const config = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8'))
const metadata = JSON.parse(run('cargo', ['metadata', '--no-deps', '--format-version', '1']))
const architecture = run('dpkg', ['--print-architecture'])
const artifact = join(
  metadata.target_directory,
  'release/bundle/deb',
  `ParleyDesktop_${config.version}_${architecture}.deb`,
)
assert.equal(run('dpkg-deb', ['--field', artifact, 'Package']), 'parley-desktop')
assert.equal(run('dpkg-deb', ['--field', artifact, 'Version']), config.version.replace('-', '~'))
assert.equal(run('dpkg-deb', ['--field', artifact, 'Conflicts']), 'parley (<< 0.5.0)')
assert.equal(run('dpkg-deb', ['--field', artifact, 'Replaces']), 'parley (<< 0.5.0)')
assert.equal(run('dpkg-deb', ['--field', artifact, 'Provides']), '')
run('dpkg', ['--compare-versions', '4:23.08.5-0ubuntu3', 'ge', '0.5.0'])
const stage = mkdtempSync(join(tmpdir(), 'parley-deb-check-'))
try {
  run('dpkg-deb', ['--raw-extract', artifact, stage])
  const entries = readdirSync(stage, { recursive: true, withFileTypes: true })
  const files = entries.filter((e) => e.isFile()).map((e) => join(e.parentPath, e.name))
  assert(!files.includes(join(stage, 'usr/bin/parley')), 'Must not overwrite KDE executable')
  assert(!files.some((f) => f.includes('/usr/share/doc/parley/')), 'Must isolate package docs')
  assert(!files.some((f) => f.endsWith('/apps/parley.png')), 'Must not overwrite KDE icons')
  for (const name of ['parley', 'parley-cli']) {
    assert(files.includes(join(stage, 'usr/lib/parley-desktop', name)), 'CLI needs sibling GUI')
    const launcher = name === 'parley' ? 'parley-desktop' : name
    assert.equal(
      readFileSync(join(stage, 'usr/bin', launcher), 'utf8'),
      `#!/bin/sh\nexec /usr/lib/parley-desktop/${name} "$@"\n`,
    )
  }
  for (const file of files.filter((f) => f.endsWith('.desktop'))) {
    const desktop = readFileSync(file, 'utf8')
    assert.match(desktop, /^Icon=parley-desktop$/m)
    assert.match(desktop, /^Exec=parley-(desktop|cli)(\s|$)/m)
  }
  for (const line of readFileSync(join(stage, 'DEBIAN/md5sums'), 'utf8').trim().split('\n')) {
    const [checksum, path] = line.split('  ')
    assert.equal(
      createHash('md5')
        .update(readFileSync(join(stage, path)))
        .digest('hex'),
      checksum,
    )
  }
  console.log('Debian identity, KDE-safe paths, launchers, icons and payload checksums verified')
} finally {
  rmSync(stage, { recursive: true, force: true })
}
