import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

// KDE already owns the Debian package, executable and icon name "parley".
// Keep our binaries together privately so the CLI can find its sibling GUI.
if (process.platform !== 'linux') throw new Error('Debian packaging requires Linux.')
const root = fileURLToPath(new URL('../', import.meta.url))
const run = (command, args) => execFileSync(command, args, { cwd: root, encoding: 'utf8' }).trim()
const config = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'))
const metadata = JSON.parse(
  run('cargo', ['metadata', '--no-deps', '--format-version', '1', '--locked']),
)
const architecture = run('dpkg', ['--print-architecture'])
const bundle = join(metadata.target_directory, 'release/bundle/deb')
const raw = join(bundle, `${config.productName}_${config.version}_${architecture}.deb`)
const artifact = join(bundle, `ParleyDesktop_${config.version}_${architecture}.deb`)
const input = existsSync(raw) ? raw : artifact
const version = config.version.replace('-', '~')
const actual = run('dpkg-deb', ['--field', input, 'Version'])
if (![config.version, version].includes(actual))
  throw new Error('Unexpected Debian package version.')
const packageName = run('dpkg-deb', ['--field', input, 'Package'])
if (!['parley', 'parley-desktop'].includes(packageName))
  throw new Error('Unexpected Debian package identity.')
if (version.includes('~')) {
  run('dpkg', ['--compare-versions', version, 'lt', config.version.split('-')[0]])
}
const stage = mkdtempSync(join(tmpdir(), 'parley-deb-'))
const output = join(bundle, `.${basename(artifact)}.tmp`)
try {
  run('dpkg-deb', ['--raw-extract', input, stage])
  chmodSync(stage, 0o755)
  if (packageName === 'parley') {
    const privateBin = join(stage, 'usr/lib/parley-desktop')
    mkdirSync(privateBin, { recursive: true })
    for (const binary of ['parley', 'parley-cli']) {
      renameSync(join(stage, 'usr/bin', binary), join(privateBin, binary))
      const launcher = join(stage, 'usr/bin', binary === 'parley' ? 'parley-desktop' : binary)
      writeFileSync(launcher, `#!/bin/sh\nexec /usr/lib/parley-desktop/${binary} "$@"\n`)
      chmodSync(launcher, 0o755)
    }
    renameSync(join(stage, 'usr/share/doc/parley'), join(stage, 'usr/share/doc/parley-desktop'))
    const icons = join(stage, 'usr/share/icons/hicolor')
    for (const size of readdirSync(icons)) {
      const old = join(icons, size, 'apps/parley.png')
      if (existsSync(old)) renameSync(old, join(dirname(old), 'parley-desktop.png'))
    }
    const applications = join(stage, 'usr/share/applications')
    for (const entry of readdirSync(applications)) {
      if (!entry.endsWith('.desktop')) continue
      const path = join(applications, entry)
      writeFileSync(
        path,
        readFileSync(path, 'utf8')
          .replace(/^Exec=parley(?=\s|$)/m, 'Exec=parley-desktop')
          .replace(/^Icon=parley$/m, 'Icon=parley-desktop'),
      )
    }
  }
  const control = join(stage, 'DEBIAN/control')
  let text = readFileSync(control, 'utf8')
    .replace(/^Package: .+$/m, 'Package: parley-desktop')
    .replace(/^Version: .+$/m, `Version: ${version}`)
  // Migrate only our old 0.x packages. Ubuntu's KDE Parley has epoch 4 and
  // can coexist; never claim to Provide/Replace the distribution application.
  for (const field of ['Conflicts', 'Replaces']) {
    const value = `${field}: parley (<< 0.5.0)`
    text = new RegExp(`^${field}: .+$`, 'm').test(text)
      ? text.replace(new RegExp(`^${field}: .+$`, 'm'), value)
      : `${text.trimEnd()}\n${value}\n`
  }
  writeFileSync(control, text)
  const files = readdirSync(stage, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && !entry.parentPath.startsWith(join(stage, 'DEBIAN')))
    .map((entry) => join(entry.parentPath, entry.name).slice(stage.length + 1))
    .sort()
  // Recompute after relocation: stale md5sums would report missing files.
  writeFileSync(
    join(stage, 'DEBIAN/md5sums'),
    files
      .map(
        (file) =>
          `${createHash('md5')
            .update(readFileSync(join(stage, file)))
            .digest('hex')}  ${file}\n`,
      )
      .join(''),
  )
  run('dpkg-deb', ['--root-owner-group', '--build', stage, output])
  if (run('dpkg-deb', ['--field', output, 'Package']) !== 'parley-desktop')
    throw new Error('Debian package identity normalization failed.')
  renameSync(output, artifact)
  if (raw !== artifact) rmSync(raw, { force: true })
} finally {
  rmSync(stage, { recursive: true, force: true })
  rmSync(output, { force: true })
}
const checksum = createHash('sha256').update(readFileSync(artifact)).digest('hex')
writeFileSync(join(bundle, 'SHA256SUMS'), `${checksum}  ${basename(artifact)}\n`)
console.log(
  `Debian package ready: ${artifact}\nPackage: parley-desktop\nVersion: ${version}\nSHA256: ${checksum}`,
)
