import { spawnSync } from 'node:child_process'

let failed = false

function check(label, command, args, required = true) {
  const result = spawnSync(command, args, { encoding: 'utf8', timeout: 15_000 })
  const available = !result.error && result.status === 0
  const summary = available
    ? result.stdout.trim().split('\n').join(', ')
    : result.error?.message || result.stderr.trim() || `exit ${result.status}`
  console.log(`${available ? 'OK' : required ? 'FAIL' : 'OPTIONAL'} ${label}: ${summary}`)
  if (!available && required) failed = true
}

const [major, minor] = process.versions.node.split('.').map(Number)
const supportedNode = major === 24 && minor >= 12
console.log(`${supportedNode ? 'OK' : 'FAIL'} Node.js: ${process.version} (requires 24.12+)`)
if (!supportedNode) failed = true
check('Rust', 'rustc', ['--version'])
check('Cargo', 'cargo', ['--version'])
check('Git', 'git', ['--version'])

if (process.platform === 'linux') {
  check('Linux desktop libraries', 'pkg-config', ['--modversion', 'gtk+-3.0', 'webkit2gtk-4.1'])
}

// Version probe only. Browser UI development does not need Codex.
if (process.platform !== 'win32') {
  check('Codex on PATH (reference only)', 'codex', ['--version'], false)
} else {
  console.log(
    'OPTIONAL Codex: run codex --version in your terminal before connecting desktop chat.',
  )
  console.log('NOTE Windows: also install the C++ Build Tools and WebView2 runtime.')
}

console.log(
  'NOTE Parley uses the Codex executable path saved in connection settings, not this PATH probe.',
)

if (process.platform === 'darwin') check('Xcode command line tools', 'xcode-select', ['-p'])
console.log('Full platform prerequisites: https://v2.tauri.app/start/prerequisites/')
process.exitCode = failed ? 1 : 0
