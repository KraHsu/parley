// Requires the Vite dev server and a disposable Chrome CDP page. IPC is mocked for layout tests.
import assert from 'node:assert/strict'
import { writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
const cdp = process.env.PARLEY_CDP_URL ?? 'http://127.0.0.1:19350'
const pages = await (await fetch(`${cdp}/json/list`)).json()
const socket = new WebSocket(pages.find((page) => page.type === 'page').webSocketDebuggerUrl)
await new Promise((resolve, reject) => {
  socket.addEventListener('open', resolve, { once: true })
  socket.addEventListener('error', reject, { once: true })
})
let sequence = 0
const requests = new Map()
const errors = []
socket.addEventListener('message', ({ data }) => {
  const message = JSON.parse(data)
  if (message.method === 'Runtime.exceptionThrown') errors.push(message.params)
  if (message.id && requests.has(message.id)) {
    const request = requests.get(message.id)
    requests.delete(message.id)
    clearTimeout(request.timeout)
    if (message.error) request.reject(new Error(JSON.stringify(message.error)))
    else request.resolve(message.result)
  }
})
function call(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++sequence
    const timeout = setTimeout(() => reject(new Error(`Timed out: ${method}`)), 10000)
    requests.set(id, { resolve, reject, timeout })
    socket.send(JSON.stringify({ id, method, params }))
  })
}
async function evaluate(expression) {
  const result = await call('Runtime.evaluate', {
    expression,
    returnByValue: true,
    awaitPromise: true,
  })
  if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails))
  return result.result.value
}
async function waitFor(expression) {
  for (let attempt = 0; attempt < 60; attempt++) {
    if (await evaluate(expression)) return
    await new Promise((resolve) => setTimeout(resolve, 100))
  }
  throw new Error(`Condition failed: ${expression}`)
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
async function screenshot(name) {
  const shot = await call('Page.captureScreenshot', { format: 'png' })
  await writeFile(join(tmpdir(), `parley-ui-${name}.png`), Buffer.from(shot.data, 'base64'))
}
async function viewport(width, height) {
  await call('Emulation.setDeviceMetricsOverride', {
    width,
    height,
    deviceScaleFactor: 1,
    mobile: false,
  })
  await waitFor(`innerWidth === ${width} && innerHeight === ${height}`)
  await sleep(70)
}

try {
  await call('Runtime.enable')
  await call('Page.enable')
  await call('Page.navigate', { url: 'http://127.0.0.1:1420' })
  await waitFor(`document.querySelector('.workspace')`)
  await evaluate(`(async()=>{
    const {createApp,h,reactive}=await import('/node_modules/.vite/deps/vue.js')
    const {default:StudyText}=await import('/src/features/vocabulary/StudyText.vue')
    const {useVocabularyStore}=await import('/src/features/vocabulary/store.ts')
    window.words=useVocabularyStore();words.initialized=true;words.start=async(source)=>{window.captured=source}
    window.fixture=reactive({text:'🌍 café café 你好 مرحبا',origin:{sourceKind:'terminal',conversationId:null,messageId:null,threadId:'thread-a',turnId:'turn',itemId:'item-a',role:'assistant',truncated:false},language:'fr'})
    const host=document.createElement('div');host.id='word-fixture';host.style='position:fixed;inset:0;background:white;padding:40px;z-index:999';document.body.append(host)
    createApp({render:()=>h(StudyText,fixture)}).mount(host)
  })()`)
  await waitFor(`document.querySelector('#word-fixture .study-body')`)
  async function select(start, end) {
    await evaluate(
      `(()=>{const body=document.querySelector('#word-fixture .study-body');body.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true}));const range=document.createRange();range.setStart(body.firstChild,${start});range.setEnd(body.firstChild,${end});const selection=getSelection();selection.removeAllRanges();selection.addRange(range);body.dispatchEvent(new PointerEvent('pointerup',{bubbles:true}));})()`,
    )
  }
  await select(8, 12)
  await evaluate(`document.querySelector('#word-fixture .study-actions button').click()`)
  let captured = await evaluate('JSON.parse(JSON.stringify(captured))')
  assert.equal(captured.selectedText, 'café')
  assert.equal(captured.start, 7)
  assert.equal(captured.end, 11)
  await evaluate(
    `fixture.text='replacement';fixture.origin={...fixture.origin,threadId:'thread-b',itemId:'item-b'}`,
  )
  await sleep(50)
  await evaluate(`document.querySelector('#word-fixture .study-actions button').click()`)
  captured = await evaluate('JSON.parse(JSON.stringify(captured))')
  assert.equal(captured.threadId, 'thread-a')
  assert.equal(captured.snapshot, '🌍 café café 你好 مرحبا')
  await evaluate(
    `[...document.querySelectorAll('#word-fixture button')].find(b=>b.textContent.includes('结束选择')).click()`,
  )
  await sleep(30)
  assert.equal(
    await evaluate(`document.querySelector('#word-fixture .study-body').textContent`),
    'replacement',
  )
  await evaluate(`fixture.text='cafe\u0301';`)
  await sleep(30)
  await select(0, 4)
  assert.match(
    await evaluate(`document.querySelector('#word-fixture .inline-error').textContent`),
    /组合字符/,
  )
  await evaluate(`document.querySelector('#word-fixture').remove()`)
  await viewport(420, 820)
  await call('Page.navigate', { url: 'http://127.0.0.1:1420/?mode=tutor' })
  await waitFor(`document.querySelector('.tutor-app')`)
  assert.equal(await evaluate(`document.documentElement.scrollWidth`), 420)
  await evaluate(`(async()=>{
    const {useVocabularyStore}=await import('/src/features/vocabulary/store.ts');window.words=useVocabularyStore()
    window.demoEntry={id:'entry',language:'fr',languageLabel:'Français',kind:'phrase',text:'Bonjour café 🌍 你好 مرحبا',meaning:'你好，很高兴再次见到你。',meaningLanguage:'zh-CN',note:'原句与自己的理解',revision:1,createdAt:1,updatedAt:1,deletedAt:null,occurrences:[],tags:['会话'],cards:[]}
    const card={id:'card',entryId:'entry',direction:'recognition',stage:0,dueAt:1,lastReviewedAt:null,suspended:false,scheduleVersion:1,revision:1}
    window.gradeCalls=0
    window.__TAURI_INTERNALS__={invoke:async(command,args)=>{
      if(command==='vocabulary_review_queue')return {items:[{card,entry:demoEntry}],dueCount:0,newCount:1,nextDueAt:null,completedToday:0,lastReviewId:null}
      if(command==='vocabulary_review_grade'){gradeCalls++;await new Promise(r=>setTimeout(r,80));return {card:{...card,revision:2},reviewId:'review'}}
      if(command==='vocabulary_list')return {entries:[demoEntry],total:1,languages:['fr'],tags:['会话'],activeCount:1}
      if(command==='vocabulary_load_drafts')return []
      return null
    }}
    document.querySelector('dialog[open]')?.close();words.initialized=true;words.selected=demoEntry;words.activeTab='words';words.wordFocusRequest++
  })()`)
  await sleep(60)
  assert.equal(await evaluate(`document.documentElement.scrollWidth`), 420)
  await screenshot('vocabulary-detail-420')
  await evaluate(`words.edit(demoEntry)`)
  await waitFor(`document.querySelector('.vocabulary-editor[open]')`)
  const footer = await evaluate(
    `document.querySelector('.vocabulary-editor .dialog-footer').getBoundingClientRect().toJSON()`,
  )
  assert.ok(footer.bottom <= 820 && footer.left >= 0 && footer.right <= 420)
  await screenshot('vocabulary-editor-420')
  await evaluate(`words.editorOpen=false;words.activeTab='review'`)
  await sleep(80)
  await evaluate(
    `[...document.querySelectorAll('.review-panel button')].find(b=>b.textContent.includes('开始复习')).click()`,
  )
  await waitFor(`document.querySelector('.review-card')`)
  await evaluate(
    `[...document.querySelectorAll('.review-panel button')].find(b=>b.textContent.includes('显示答案')).click()`,
  )
  await sleep(40)
  assert.equal(await evaluate(`document.documentElement.scrollWidth`), 420)
  await screenshot('vocabulary-review-420')
  await evaluate(
    `document.querySelector('.review-grades button:last-child').click();document.querySelector('.review-grades button:last-child').click()`,
  )
  await sleep(140)
  assert.equal(await evaluate('gradeCalls'), 1)

  assert.equal(errors.length, 0, JSON.stringify(errors))
  console.log(
    'PASS: actual DOM selection offsets, repeated words, frozen text/source across updates, grapheme rejection, 420px detail/editor/review layouts, and duplicate-click grading guard.',
  )
} finally {
  socket.close()
}
