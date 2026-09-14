import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig(({ mode, command }) => ({
  base: mode === 'web' ? process.env.PARLEY_WEB_BASE || './' : '/',
  plugins: [
    vue(),
    ...(mode === 'web'
      ? [
          {
            name: 'parley-web-entry',
            transformIndexHtml: {
              order: 'pre' as const,
              handler(html: string) {
                const policy =
                  command === 'build'
                    ? `<meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' https: http://localhost:* http://127.0.0.1:* http://[::1]:*; img-src 'self' data:; object-src 'none'; base-uri 'self'; form-action 'none'" />`
                    : ''
                return html
                  .replace('<head>', `<head>${policy}`)
                  .replace('/src/main.ts', '/src/web/main.ts')
                  .replace(
                    '<title>Parley</title>',
                    `<meta name="referrer" content="no-referrer" />
    <meta name="description" content="Parley：使用自己的 API 练习语言、理解语法、收藏词句。学习数据保存在你的浏览器。" />
    <title>Parley · 语言学习</title>`,
                  )
              },
            },
          },
        ]
      : []),
  ],
  clearScreen: false,
  server: {
    host: '127.0.0.1',
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**', '**/target/**'] },
  },
  build: {
    outDir: mode === 'web' ? 'dist-web' : 'dist',
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari14',
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
  },
}))
