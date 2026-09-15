import { readFile } from 'node:fs/promises'
import { test, expect, type Page } from '@playwright/test'
async function ready(page: Page) {
  await page.goto('./')
  await page.getByRole('button', { name: '设置', exact: true }).click()
  const form = page.locator('form.new-profile')
  await form.getByLabel('配置名称').fill('Fixture API')
  await form.getByLabel('API 服务地址').fill('http://127.0.0.1:4179/v1')
  await form.getByLabel('API Key').fill('fixture-key')
  await form.getByRole('button', { name: '保存服务' }).click()
  await expect(page.getByText('服务已添加。请为两个面板分别选择模型。')).toBeVisible()
  await page.getByRole('button', { name: '完成', exact: true }).click()
  for (const pane of ['main', 'tutor']) {
    await page.getByLabel(`${pane} 模型`, { exact: true }).fill('fixture')
    await page.getByLabel(`${pane} 模型`, { exact: true }).press('Tab')
  }
}
async function addWord(page: Page) {
  await page.getByRole('button', { name: /^词句/ }).click()
  await page.getByRole('button', { name: '＋ 添加词句' }).click()
  const editor = page.getByRole('dialog', { name: '收藏词句' })
  await editor.getByLabel('词句', { exact: true }).fill('break the ice')
  await editor.getByLabel('释义', { exact: true }).fill('打破冷场')
  return editor
}
test('saves a word, rejects an oversized tag, reloads and restores a downloaded backup', async ({
  page,
}) => {
  await ready(page)
  const editor = await addWord(page)
  await editor.getByLabel('标签', { exact: true }).fill('x'.repeat(101))
  await editor.getByRole('button', { name: '保存词句' }).click()
  await expect(editor.getByRole('alert')).toContainText('100')
  await editor.getByLabel('标签', { exact: true }).fill('daily, grammar')
  await editor.getByRole('button', { name: '保存词句' }).click()
  await expect(editor).not.toBeVisible()
  await page.reload()
  await page.getByRole('button', { name: /^词句/ }).click()
  await expect(page.getByRole('heading', { name: 'break the ice' })).toBeVisible()
  await page.getByText('备份', { exact: true }).click()
  const download = page.waitForEvent('download')
  await page.getByRole('button', { name: '导出 Web 备份' }).click()
  const file = await download
  await page.getByLabel('导入 Web 备份文件').setInputFiles((await file.path())!)
  const restore = page.getByRole('dialog', { name: '恢复 Web 备份' })
  await restore.getByRole('button', { name: '确认恢复' }).click()
  await expect(restore).not.toBeVisible()
  await expect(page.getByText('备份已恢复。API Key 不在备份中，请重新填写。')).toBeVisible()
})
test('keeps the restore preview and original durable state after an IndexedDB failure', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const original = IDBObjectStore.prototype.put
    IDBObjectStore.prototype.put = function (...args: Parameters<typeof original>) {
      const request = original.apply(this, args)
      if ((window as unknown as { failSave: boolean }).failSave && this.name === 'words')
        this.transaction.abort()
      return request
    }
  })
  await ready(page)
  const editor = await addWord(page)
  await editor.getByRole('button', { name: '保存词句' }).click()
  await expect(editor).not.toBeVisible()
  const state = (await page.evaluate(async () => {
    const db = await new Promise<IDBDatabase>((resolve) => {
      const req = indexedDB.open('parley-web')
      req.onsuccess = () => resolve(req.result)
    })
    const tx = db.transaction(['workspace', 'conversations'])
    const get = (req: IDBRequest) =>
      new Promise<unknown>((resolve) => {
        req.onsuccess = () => resolve(req.result)
      })
    const [meta, conversations] = await Promise.all([
      get(tx.objectStore('workspace').get('current')),
      get(tx.objectStore('conversations').getAll()),
    ])
    db.close()
    return { meta, conversations }
  })) as { meta: { settings: unknown; active: unknown }; conversations: { id: string }[] }
  const backup = {
    app: 'parley-web',
    state: {
      version: 1,
      settings: state.meta.settings,
      active: state.meta.active,
      profiles: [],
      conversations: state.conversations.map((c) => ({ ...c, profileId: '', messages: [] })),
      words: [
        {
          id: 'replacement',
          text: 'replacement',
          language: 'en',
          meaning: '',
          note: '',
          tags: [],
          source: null,
          createdAt: 1,
          review: null,
        },
      ],
    },
  }
  await page.getByLabel('导入 Web 备份文件').setInputFiles({
    name: 'backup.json',
    mimeType: 'application/json',
    buffer: Buffer.from(JSON.stringify(backup)),
  })
  await page.evaluate(() => {
    ;(window as unknown as { failSave: boolean }).failSave = true
  })
  const restore = page.getByRole('dialog', { name: '恢复 Web 备份' })
  await restore.getByRole('button', { name: '确认恢复' }).click()
  await expect(restore.getByRole('alert')).toContainText('本地保存失败')
  await expect(restore).toBeVisible()
  await restore.getByRole('button', { name: '取消', exact: true }).click()
  await expect(page.getByRole('heading', { name: 'break the ice' })).toBeVisible()
  await page.reload()
  await page.getByRole('button', { name: /^词句/ }).click()
  await expect(page.getByRole('heading', { name: 'break the ice' })).toBeVisible()
})
test('keeps tutor context when changing task and does not send IME confirmation Enter', async ({
  page,
}) => {
  await ready(page)
  const input = page.getByLabel('tutor 输入', { exact: true })
  await input.fill('请解释 break the ice')
  await input.dispatchEvent('compositionstart')
  await input.press('Enter')
  await expect(page.locator('.web-message.assistant')).toHaveCount(0)
  await input.dispatchEvent('compositionend')
  await input.press('Enter')
  await expect(page.locator('.web-message.assistant')).toContainText('friendly hello')
  await expect(page.getByRole('button', { name: '停止', exact: true })).toHaveCount(0)
  await page.getByLabel('辅导任务', { exact: true }).selectOption('translate')
  await expect(input).toHaveAttribute('placeholder', '粘贴需要翻译的句子…')
  await input.fill('翻译一下刚才那句话')
  await input.press('Enter')
  await expect(page.locator('.web-message.assistant').last()).toContainText('刚才那句话的翻译')
  await expect(page.locator('.web-message.assistant')).toHaveCount(2)
})
async function seedLegacy(page: Page, count: number, oversizedTag = false) {
  await page.goto('./_seed')
  await page.evaluate(
    async ({ count, oversizedTag }) => {
      const conversation = (pane: string) => ({
        id: pane,
        pane,
        title: '新的对话',
        profileId: '',
        model: '',
        target: 'en',
        native: 'zh-CN',
        draft: '',
        messages: [],
        createdAt: 1,
      })
      const state = {
        version: 1,
        settings: { target: 'en', native: 'zh-CN' },
        profiles: [],
        conversations: [conversation('main'), conversation('tutor')],
        active: { main: 'main', tutor: 'tutor' },
        words: Array.from({ length: count }, (_, i) => ({
          id: `word-${i}`,
          text: `word ${i}`,
          language: 'en',
          meaning: `meaning ${i}`,
          note: '',
          tags: [oversizedTag ? 'x'.repeat(101) : 'daily'],
          source: null,
          createdAt: i,
          review: null,
        })),
      }
      await new Promise<void>((resolve, reject) => {
        const request = indexedDB.open('parley-web', 1)
        request.onupgradeneeded = () => request.result.createObjectStore('workspace')
        request.onsuccess = () => {
          const db = request.result,
            tx = db.transaction('workspace', 'readwrite')
          tx.objectStore('workspace').put({ revision: 1, state }, 'current')
          tx.oncomplete = () => {
            db.close()
            resolve()
          }
          tx.onerror = () => reject(tx.error)
        }
      })
    },
    { count, oversizedTag },
  )
  await page.goto('./')
}
test('recovers a legacy oversized tag and exports the untouched original first', async ({
  page,
}) => {
  await seedLegacy(page, 1, true)
  await expect(page.getByRole('button', { name: '导出原始数据' })).toBeVisible()
  const download = page.waitForEvent('download')
  await page.getByRole('button', { name: '修复过长标签' }).click()
  expect((await download).suggestedFilename()).toContain('before-repair')
  await expect(page.getByText('已缩短 1 个过长标签，原始数据已导出。')).toBeVisible()
  await page.reload()
  await page.getByRole('button', { name: /^词句/ }).click()
  await expect(page.getByRole('heading', { name: 'word 0', exact: true })).toBeVisible()
})
test('paginates a large word list and searches across all pages', async ({ page }) => {
  await seedLegacy(page, 125)
  await page.getByRole('button', { name: /^词句/ }).click()
  await expect(page.locator('.word-grid > article')).toHaveCount(50)
  await page.getByRole('button', { name: '下一页' }).click()
  await expect(page.getByRole('heading', { name: 'word 50', exact: true })).toBeVisible()
  await page.getByRole('button', { name: '下一页' }).click()
  await expect(page.locator('.word-grid > article')).toHaveCount(25)
  await page.getByLabel('搜索词句', { exact: true }).fill('meaning 1')
  await expect(page.getByRole('heading', { name: 'word 1', exact: true })).toBeVisible()
})

test('imports desktop learning data, edits and exports it without losing history, keys or chats', async ({
  page,
}) => {
  await ready(page)
  await page.getByLabel('main 输入', { exact: true }).fill('A draft to preserve')
  await page.getByRole('button', { name: /^词句/ }).click()
  await page.getByText('词句迁移 · Web ↔ 桌面', { exact: true }).click()
  await page.getByLabel('导入词句 JSON 文件').setInputFiles('fixtures/learning-exchange.json')
  const preview = page.getByRole('dialog', { name: '导入词句预览' })
  await expect(preview).toContainText('新增 2')
  await preview.getByRole('button', { name: '确认导入词句' }).click()
  await expect(preview).not.toBeVisible()
  await expect(page.getByRole('heading', { name: 'Ça marche', exact: true })).toBeVisible()
  await page.getByRole('button', { name: '设置', exact: true }).click()
  await expect(page.getByLabel('Fixture API API Key', { exact: true })).toHaveValue('fixture-key')
  await page.getByRole('button', { name: '完成', exact: true }).click()
  const card = page
    .locator('.word-card')
    .filter({ has: page.getByRole('heading', { name: 'Ça marche', exact: true }) })
  await card.getByRole('button', { name: '编辑', exact: true }).click()
  const editor = page.getByRole('dialog', { name: '收藏词句' })
  await expect(editor).toContainText('2 个来源 · 2 张卡片 · 2 次复习')
  await editor.getByLabel('我的注释', { exact: true }).fill('Edited on Web')
  await editor.getByRole('button', { name: '保存词句', exact: true }).click()
  await expect(editor).not.toBeVisible()
  await page.getByRole('button', { name: '回收站 · 1', exact: true }).click()
  await expect(page.getByRole('heading', { name: 'à bientôt', exact: true })).toBeVisible()
  const downloaded = page.waitForEvent('download')
  await page.getByRole('button', { name: '导出词句 JSON', exact: true }).click()
  const file = await downloaded
  const exported = JSON.parse(await readFile((await file.path())!, 'utf8'))
  expect(exported.version).toBe(3)
  expect(exported.entries).toHaveLength(2)
  expect(exported.entries[0].fields.note).toBe('Edited on Web')
  expect(exported.entries[0].occurrences).toHaveLength(2)
  expect(exported.entries[0].cards).toHaveLength(2)
  expect(exported.entries[0].reviews).toHaveLength(2)
  expect(exported.entries[1].deletedAt).not.toBeNull()
  expect(JSON.stringify(exported)).not.toContain('fixture-key')
  const drafts = await page.evaluate(async () => {
    const db = await new Promise<IDBDatabase>((resolve) => {
      const req = indexedDB.open('parley-web')
      req.onsuccess = () => resolve(req.result)
    })
    const records = await new Promise<{ draft: string }[]>((resolve) => {
      const req = db.transaction('conversations').objectStore('conversations').getAll()
      req.onsuccess = () => resolve(req.result)
    })
    db.close()
    return records.map((c) => c.draft)
  })
  expect(drafts).toContain('A draft to preserve')
  await page.reload()
  await page.getByRole('button', { name: /^词句/ }).click()
  await page.getByText('词句迁移 · Web ↔ 桌面', { exact: true }).click()
  await page.getByLabel('导入词句 JSON 文件').setInputFiles('fixtures/learning-exchange.json')
  await expect(preview).toContainText('重复 2')
  await preview.getByRole('button', { name: '确认导入词句' }).click()
  await expect(page.getByText('Edited on Web', { exact: true })).toBeVisible()
})

test('failed learning import keeps original words and the import preview', async ({ page }) => {
  await page.addInitScript(() => {
    const original = IDBObjectStore.prototype.put
    IDBObjectStore.prototype.put = function (...args: Parameters<typeof original>) {
      const request = original.apply(this, args)
      if ((window as unknown as { failSave: boolean }).failSave && this.name === 'words')
        this.transaction.abort()
      return request
    }
  })
  await ready(page)
  const editor = await addWord(page)
  await editor.getByRole('button', { name: '保存词句' }).click()
  await expect(editor).not.toBeVisible()
  await page.getByText('词句迁移 · Web ↔ 桌面', { exact: true }).click()
  await page.getByLabel('导入词句 JSON 文件').setInputFiles('fixtures/learning-exchange.json')
  const preview = page.getByRole('dialog', { name: '导入词句预览' })
  await expect(preview).toBeVisible()
  await page.evaluate(() => {
    ;(window as unknown as { failSave: boolean }).failSave = true
  })
  await preview.getByRole('button', { name: '确认导入词句' }).click()
  await expect(preview.getByRole('alert')).toContainText('本地保存失败')
  await expect(preview).toBeVisible()
  await page.reload()
  await page.getByRole('button', { name: /^词句/ }).click()
  await expect(page.getByRole('heading', { name: 'break the ice', exact: true })).toBeVisible()
  await expect(page.getByRole('heading', { name: 'Ça marche', exact: true })).toHaveCount(0)
})

test('practices production without revealing the expression, retries an aborted grade and preserves history after reload', async ({
  page,
}) => {
  await page.addInitScript(() => {
    const original = IDBObjectStore.prototype.put
    IDBObjectStore.prototype.put = function (...args: Parameters<typeof original>) {
      const request = original.apply(this, args)
      if ((window as unknown as { failSave: boolean }).failSave && this.name === 'words')
        this.transaction.abort()
      return request
    }
  })
  await ready(page)
  const editor = await addWord(page)
  await editor.getByRole('button', { name: '保存词句' }).click()
  await page.getByRole('button', { name: '加入表达复习', exact: true }).click()
  await expect(page.getByRole('button', { name: '暂停表达复习' })).toBeVisible()
  await page.getByRole('button', { name: /^复习 ·/ }).click()
  await page.getByLabel('复习方向').selectOption('production')
  await expect(page.getByRole('heading', { name: '打破冷场', exact: true })).toBeVisible()
  await expect(page.getByText('break the ice', { exact: true })).not.toBeVisible()
  await page.getByLabel('先试着回忆（可选）').fill('break the ice')
  await page.getByRole('button', { name: '显示答案' }).click()
  await expect(page.getByText('break the ice', { exact: true })).toBeVisible()
  await page.evaluate(() => {
    ;(window as unknown as { failSave: boolean }).failSave = true
  })
  await page.getByRole('button', { name: /^记住了/ }).click()
  await expect(page.locator('.review-area').getByRole('alert')).toBeVisible()
  await expect(page.getByRole('heading', { name: '打破冷场', exact: true })).toBeVisible()
  await page.evaluate(() => {
    ;(window as unknown as { failSave: boolean }).failSave = false
  })
  await page.getByRole('button', { name: /^记住了/ }).click()
  await expect(page.getByRole('heading', { name: '这一轮完成了。' })).toBeVisible()
  await page.reload()
  await page.getByRole('button', { name: /^词句/ }).click()
  await page.getByRole('button', { name: /^复习 ·/ }).click()
  await page.getByRole('button', { name: '撤销最近一次评分' }).click()
  await expect(page.getByRole('heading', { name: '打破冷场', exact: true })).toBeVisible()
  await page.getByRole('button', { name: '显示答案' }).click()
  await page.getByRole('button', { name: /^吃力/ }).click()
  // Open the existing migration control and inspect the file users receive.
  await page.getByText('词句迁移 · Web ↔ 桌面', { exact: true }).click()
  const download = page.waitForEvent('download')
  await page.getByRole('button', { name: '导出词句 JSON', exact: true }).click()
  const file = await download
  const data = JSON.parse(await readFile((await file.path())!, 'utf8'))
  expect(data.entries[0].cards[0].direction).toBe('production')
  expect(data.entries[0].reviews).toHaveLength(2)
  expect(
    data.entries[0].reviews.filter((r: { undoneAt: number | null }) => r.undoneAt !== null),
  ).toHaveLength(1)
  expect(
    data.entries[0].reviews.find((r: { undoneAt: number | null }) => r.undoneAt === null).rating,
  ).toBe('hard')
})
