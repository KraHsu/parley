<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import type { WebWorkspace } from './store'
import type { Word } from './types'
const props = defineProps<{ w: WebWorkspace }>()
const emit = defineEmits<{ edit: [word: Word]; source: [word: Word]; explain: [word: Word] }>()
const search = ref(''),
  tab = ref('list'),
  answer = ref(false),
  now = ref(Date.now())
const clock = setInterval(() => {
  now.value = Date.now()
}, 30000)
onUnmounted(() => clearInterval(clock))
const words = computed(() =>
  props.w.state.words.filter((w) =>
    [w.text, w.meaning, w.note, ...w.tags]
      .join('\n')
      .toLocaleLowerCase()
      .includes(search.value.toLocaleLowerCase()),
  ),
)
const due = computed(() =>
  props.w.state.words
    .filter((w) => w.review && w.review.dueAt <= now.value)
    .sort((a, b) => a.review!.dueAt - b.review!.dueAt),
)
function grade(remembered: boolean) {
  if (due.value[0]) props.w.review(due.value[0], remembered)
  answer.value = false
  now.value = Date.now()
}
function showReview() {
  tab.value = 'review'
  answer.value = false
}
function enroll(word: Word) {
  if (!props.w.canEdit) return
  word.review = { stage: 0, dueAt: Date.now(), lastReviewedAt: null }
  now.value = Date.now()
}
function add() {
  emit('edit', {
    id: crypto.randomUUID(),
    text: '',
    language: props.w.state.settings.target,
    meaning: '',
    note: '',
    tags: [],
    source: null,
    createdAt: Date.now(),
    review: null,
  })
}
</script>
<template>
  <section class="web-words">
    <header class="words-heading">
      <div>
        <span class="eyebrow">MAKE WORDS YOUR OWN</span>
        <h1>
          词句收藏 <small>{{ w.state.words.length }}</small>
        </h1>
      </div>
      <button class="primary" :disabled="!w.canEdit" @click="add">＋ 添加词句</button>
    </header>
    <div class="words-toolbar">
      <div class="segmented">
        <button :class="{ active: tab === 'list' }" @click="tab = 'list'">全部词句</button
        ><button :class="{ active: tab === 'review' }" @click="showReview">
          复习 · {{ due.length }}
        </button>
      </div>
      <input
        v-if="tab === 'list'"
        v-model="search"
        aria-label="搜索词句"
        placeholder="搜索词句、释义、注释或标签"
      />
    </div>
    <div v-if="tab === 'review'" class="review-area">
      <article v-if="due[0]" class="word-card review-card">
        <span class="eyebrow">{{ due[0].language }} · 想一想它的意思</span>
        <h2>{{ due[0].text }}</h2>
        <template v-if="answer"
          ><p>{{ due[0].meaning || '还没有释义，可在词句编辑中补充。' }}</p>
          <p class="help">{{ due[0].note }}</p>
          <div class="message-actions">
            <button :disabled="!w.canEdit" @click="grade(false)">再学一次</button
            ><button class="primary" :disabled="!w.canEdit" @click="grade(true)">记住了</button>
          </div></template
        ><button v-else class="primary" @click="answer = true">显示答案</button>
      </article>
      <div v-else class="empty-chat">
        <span>✓</span>
        <h2>这一轮完成了。</h2>
        <p>在词句卡片中添加复习，或稍后回来巩固。离线也可以复习。</p>
      </div>
    </div>
    <div v-else class="word-grid">
      <article v-for="word in words" :key="word.id" class="word-card">
        <span class="eyebrow">{{ word.language }}</span>
        <h2>{{ word.text }}</h2>
        <p>{{ word.meaning || '等待你的理解与注释。' }}</p>
        <p v-if="word.note" class="word-note">{{ word.note }}</p>
        <div class="tags">
          <span v-for="tag in word.tags" :key="tag">{{ tag }}</span>
        </div>
        <blockquote v-if="word.source">
          {{ word.source.text }}<small>{{ word.source.provider }} · {{ word.source.model }}</small>
        </blockquote>
        <div class="message-actions">
          <button :disabled="!w.canEdit" @click="emit('edit', word)">编辑</button
          ><button v-if="word.source" @click="emit('source', word)">查看来源</button
          ><button :disabled="!w.ready('tutor')" @click="emit('explain', word)">请助手解释</button
          ><button v-if="!word.review" :disabled="!w.canEdit" @click="enroll(word)">加入复习</button
          ><small v-else>下次复习 {{ new Date(word.review.dueAt).toLocaleDateString() }}</small>
        </div>
      </article>
      <div v-if="!words.length" class="empty-chat">
        <span>词</span>
        <h2>{{ search ? '还没有匹配的词句。' : '让每一次对话留下收获。' }}</h2>
        <p>选中对话里的词句收藏，添加你自己的理解。</p>
      </div>
    </div>
  </section>
</template>
