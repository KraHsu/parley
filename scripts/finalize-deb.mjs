import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { chmodSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { basename, dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

// Tauri keeps SemVer's hyphen, which Debian sorts AFTER the stable version.
// Finalize only the native Linux artifact produced by package:linux.
if (process.platform !== 'linux') throw new Error('Debian packaging requires Linux.')
const root = fileURLToPath(new URL('../', import.meta.url))
const run = (command, args) => execFileSync(command, args, { cwd: root, encoding: 'utf8' }).trim()
const config = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'))
const metadata = JSON.parse(
  run('cargo', ['metadata', '--no-deps', '--format-version', '1', '--locked']),
)
const architecture = run('dpkg', ['--print-architecture'])
const artifact = join(
  metadata.target_directory,
  'release/bundle/deb',
  `${config.productName}_${config.version}_${architecture}.deb`,
)
const version = config.version.replace('-', '~')
const actual = run('dpkg-deb', ['--field', artifact, 'Version'])
if (![config.version, version].includes(actual))
  throw new Error('Unexpected Debian package version.')
if (version.includes('~')) {
  run('dpkg', ['--compare-versions', version, 'lt', config.version.split('-')[0]])
}
if (actual !== version) {
  const stage = mkdtempSync(join(tmpdir(), 'parley-deb-'))
  const output = join(dirname(artifact), `.${basename(artifact)}.tmp`)
  try {
    run('dpkg-deb', ['--raw-extract', artifact, stage])
    // mkdtemp uses 0700; do not carry that private staging mode into the archive root.
    chmodSync(stage, 0o755)
    const control = join(stage, 'DEBIAN/control')
    writeFileSync(
      control,
      readFileSync(control, 'utf8').replace(/^Version: .+$/m, `Version: ${version}`),
    )
    run('dpkg-deb', ['--root-owner-group', '--build', stage, output])
    if (run('dpkg-deb', ['--field', output, 'Version']) !== version)
      throw new Error('Debian version normalization failed.')
    renameSync(output, artifact)
  } finally {
    rmSync(stage, { recursive: true, force: true })
    rmSync(output, { force: true })
  }
}
const checksum = createHash('sha256').update(readFileSync(artifact)).digest('hex')
writeFileSync(join(dirname(artifact), 'SHA256SUMS'), `${checksum}  ${basename(artifact)}\n`)
console.log(`Debian package ready: ${artifact}\nPackage version: ${version}\nSHA256: ${checksum}`)
