<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { wordInvoke as invoke } from '../../shared/word-operations'
import { useVocabularyStore } from './store'
import { useCodexStore } from '../codex/store'
import type { ReviewCard, ReviewQueue, ReviewItem } from './types'
const vocabulary = useVocabularyStore()
const codex = useCodexStore()
const items = ref<ReviewItem[]>([])
const stats = ref<ReviewQueue | null>(null)
const limit = ref(20)
const newLimit = ref(5)
const busy = ref(false)
const error = ref('')
const started = ref(false)
const revealed = ref(false)
const recall = ref('')
const completed = ref(0)
const current = computed(() => items.value[0])
let pendingGrade: {
  requestId: string
  cardId: string
  expectedRevision: number
  rating: string
} | null = null
let pendingUndo: { requestId: string; reviewId: string } | null = null
function dayStart() {
  const d = new Date()
  d.setHours(0, 0, 0, 0)
  return d.getTime()
}
async function readQueue() {
  return invoke<ReviewQueue>('vocabulary_review_queue', {
    limit: Number(limit.value),
    newLimit: Number(newLimit.value),
    dayStart: dayStart(),
  })
}
async function refreshStats() {
  if (!vocabulary.initialized) return
  try {
    stats.value = await readQueue()
    error.value = ''
  } catch (e) {
    error.value = String(e)
  }
}
watch(
  () => vocabulary.initialized,
  (ready) => {
    if (ready) void refreshStats()
  },
  { immediate: true },
)
async function start() {
  if (busy.value) return
  busy.value = true
  error.value = ''
  try {
    const queue = await readQueue()
    stats.value = queue
    items.value = queue.items
    started.value = true
    revealed.value = false
    recall.value = ''
    completed.value = 0
    pendingGrade = null
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
async function grade(rating: string) {
  if (!current.value || busy.value || !revealed.value) return
  busy.value = true
  error.value = ''
  const card = current.value.card
  if (
    !pendingGrade ||
    pendingGrade.cardId !== card.id ||
    pendingGrade.expectedRevision !== card.revision ||
    pendingGrade.rating !== rating
  )
    pendingGrade = {
      requestId: crypto.randomUUID(),
      cardId: card.id,
      expectedRevision: card.revision,
      rating,
    }
  try {
    const result = await invoke<{ card: ReviewCard; reviewId: string }>('vocabulary_review_grade', {
      request: pendingGrade,
    })
    items.value.shift()
    completed.value++
    revealed.value = false
    recall.value = ''
    pendingGrade = null
    await refreshStats()
    if (stats.value) stats.value.lastReviewId = result.reviewId
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
async function undo() {
  const id = stats.value?.lastReviewId
  if (!id || busy.value) return
  busy.value = true
  error.value = ''
  if (pendingUndo?.reviewId !== id) pendingUndo = { requestId: crypto.randomUUID(), reviewId: id }
  try {
    await invoke('vocabulary_review_undo', { request: pendingUndo })
    pendingUndo = null
    const queue = await readQueue()
    stats.value = queue
    items.value = queue.items
    completed.value = Math.max(0, completed.value - 1)
    revealed.value = false
    recall.value = ''
    started.value = true
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
async function practice() {
  if (!current.value || !codex.ready || codex.lanes.tutor.busy) return
  const fields = current.value.entry
  vocabulary.tutorFocusRequest++
  codex.mobilePane = 'tutor'
  await codex.send(
    'tutor',
    `我想练习使用这个表达。请给一个简短情境，让我用它写一句话，再等我回答。\n${JSON.stringify({ language: fields.language, expression: fields.text, meaning: fields.meaning })}`,
    'express',
  )
}
</script>
<template>
  <section class="review-panel">
    <header class="word-detail-header">
      <h2>今日复习</h2>
      <button class="text-button" :disabled="busy" @click="refreshStats">刷新</button>
    </header>
    <p v-if="stats" class="review-stats">
      {{ stats.dueCount }} 张到期 · {{ stats.newCount }} 张新卡 · 今天已完成
      {{ stats.completedToday }} 次
    </p>
    <div v-if="!started" class="review-start">
      <p>先回忆，再看答案。忘记也没关系，下次会更快认出它。</p>
      <div class="word-form-row">
        <label>本轮最多<input v-model.number="limit" type="number" min="1" max="200" /></label
        ><label
          >其中新卡最多<input v-model.number="newLimit" type="number" min="0" :max="limit"
        /></label>
      </div>
      <button class="primary-button" :disabled="busy || !vocabulary.initialized" @click="start">
        开始复习
      </button>
      <p class="settings-help">在词句详情中补充释义，然后加入复习。普通复习离线可用。</p>
    </div>
    <article v-else-if="current" class="review-card">
      <p class="overline">
        {{ current.card.direction === 'recognition' ? '回忆含义' : '回忆目标表达' }} · 本轮已完成
        {{ completed }}
      </p>
      <h2 dir="auto">
        {{ current.card.direction === 'recognition' ? current.entry.text : current.entry.meaning }}
      </h2>
      <label
        >先试着回忆（可选）<textarea
          v-model="recall"
          dir="auto"
          rows="3"
          placeholder="在心里回答，或写在这里…"
          :disabled="busy"
        />
      </label>
      <button v-if="!revealed" class="primary-button" @click="revealed = true">显示答案</button>
      <template v-else>
        <div class="review-answer">
          <p dir="auto">
            {{
              current.card.direction === 'recognition' ? current.entry.meaning : current.entry.text
            }}
          </p>
          <blockquote v-if="current.entry.occurrences[0]" dir="auto">
            {{ current.entry.occurrences[0].snapshot }}
          </blockquote>
          <p v-if="current.entry.note" class="settings-help" dir="auto">{{ current.entry.note }}</p>
        </div>
        <p class="settings-help">同义表达也算，以你的回忆情况为准。</p>
        <div class="review-grades">
          <button :disabled="busy" @click="grade('forgot')">忘记<small>10 分钟后</small></button
          ><button :disabled="busy" @click="grade('hard')">吃力<small>1 天后</small></button
          ><button :disabled="busy" @click="grade('remembered')">
            记住<small>{{ [1, 3, 7, 14, 30][Math.min(current.card.stage, 4)] }} 天后</small>
          </button>
        </div>
        <button
          class="text-button"
          :disabled="!codex.ready || codex.lanes.tutor.busy"
          @click="practice"
        >
          请助手给一个表达练习
        </button>
      </template>
    </article>
    <div v-else class="review-finished">
      <p class="overline">A LITTLE, EVERY DAY</p>
      <h2>这一轮完成了。</h2>
      <p>本轮完成 {{ completed }} 次回忆。</p>
      <p v-if="stats?.nextDueAt" class="settings-help">
        下一次到期：{{ new Date(stats.nextDueAt).toLocaleString() }}，可以先回到对话。
      </p>
      <button class="secondary-button" @click="started = false">返回复习首页</button>
    </div>
    <button v-if="stats?.lastReviewId" class="text-button" :disabled="busy" @click="undo">
      撤销最近一次评分
    </button>
    <button v-if="started && current" class="text-button" :disabled="busy" @click="started = false">
      结束本轮，保留已完成评分
    </button>
    <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
  </section>
</template>
