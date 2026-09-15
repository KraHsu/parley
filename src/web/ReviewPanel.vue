<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { dueCards, latestReview, type Direction } from './review'
import type { WebWorkspace } from './store'
import type { LearningReview } from '../shared/learning-exchange'
const props = defineProps<{ w: WebWorkspace }>()
const direction = ref<Direction | ''>(''),
  answer = ref(false),
  recall = ref(''),
  error = ref(''),
  busy = ref(false),
  now = ref(Date.now())
const timer = setInterval(() => {
  now.value = Date.now()
}, 30000)
onUnmounted(() => clearInterval(timer))
const due = computed(() => dueCards(props.w.state.words, now.value, direction.value || undefined))
const current = computed(() => due.value[0])
const last = computed(() => latestReview(props.w.state.words))
const missing = computed(
  () =>
    props.w.state.words.filter((w) => !w.meaning.trim() && w.learning?.entry.deletedAt == null)
      .length,
)
watch(
  () => current.value?.card.id,
  () => {
    answer.value = false
    recall.value = ''
  },
)
watch(direction, () => {
  answer.value = false
  recall.value = ''
  error.value = ''
})
async function grade(rating: LearningReview['rating']) {
  if (!current.value || busy.value || !answer.value) return
  busy.value = true
  error.value = ''
  const { word, card } = current.value
  try {
    await props.w.review(word, rating, card.direction, card.revision)
    answer.value = false
    recall.value = ''
    now.value = Date.now()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
async function undo() {
  if (!last.value || busy.value) return
  busy.value = true
  error.value = ''
  try {
    await props.w.undoReview(last.value.review.id)
    answer.value = false
    recall.value = ''
    now.value = Date.now()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
</script>
<template>
  <div class="review-area">
    <label
      >复习方向<select v-model="direction" :disabled="busy">
        <option value="">识义与表达</option>
        <option value="recognition">识义 · 看词句想含义</option>
        <option value="production">表达 · 看含义想词句</option>
      </select></label
    >
    <p class="help">
      {{ due.length }} 张到期卡片<span v-if="missing"> · {{ missing }} 个词句需要先补充释义</span>
    </p>
    <article v-if="current" class="word-card review-card">
      <span class="eyebrow"
        >{{ current.word.language }} ·
        {{ current.card.direction === 'production' ? '回忆目标表达' : '想一想它的意思' }}</span
      >
      <h2 dir="auto">
        {{ current.card.direction === 'production' ? current.word.meaning : current.word.text }}
      </h2>
      <label
        >先试着回忆（可选）<textarea
          v-model="recall"
          dir="auto"
          rows="3"
          :disabled="busy"
          :placeholder="
            current.card.direction === 'production'
              ? '用目标语言写出这个意思…'
              : '在心里回答，或写下它的含义…'
          "
        />
      </label>
      <template v-if="answer">
        <p dir="auto">
          {{ current.card.direction === 'production' ? current.word.text : current.word.meaning }}
        </p>
        <blockquote v-if="current.word.source" dir="auto">
          {{ current.word.source.text }}
        </blockquote>
        <p class="help" dir="auto">{{ current.word.note }}</p>
        <p class="help">同义表达也算，以你的回忆情况为准。</p>
        <div class="message-actions">
          <button :disabled="!w.canEdit || busy" @click="grade('forgot')">
            再学一次 · 10 分钟
          </button>
          <button :disabled="!w.canEdit || busy" @click="grade('hard')">吃力 · 1 天</button>
          <button class="primary" :disabled="!w.canEdit || busy" @click="grade('remembered')">
            记住了 ·
            {{
              [1, 3, 7, 14, 30, 60][
                Math.min(current.card.stage, current.card.scheduleVersion === 1 ? 4 : 5)
              ]
            }}
            天
          </button>
        </div>
      </template>
      <button v-else class="primary" @click="answer = true">显示答案</button>
    </article>
    <div v-else class="empty-chat">
      <span>✓</span>
      <h2>这一轮完成了。</h2>
      <p>在词句卡片中添加复习，或稍后回来巩固。</p>
    </div>
    <button v-if="last" :disabled="!w.canEdit || busy" @click="undo">撤销最近一次评分</button>
    <p v-if="error" class="web-error" role="alert">{{ error }}</p>
  </div>
</template>
