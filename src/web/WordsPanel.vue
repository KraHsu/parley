<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import ReviewPanel from './ReviewPanel.vue'
import { dueCards, wordCards, type Direction } from './review'
import LearningExchange from './LearningExchange.vue'
import type { WebWorkspace } from './store'
import type { Word } from './types'
const props = defineProps<{ w: WebWorkspace }>()
const emit = defineEmits<{ edit: [word: Word]; source: [word: Word]; explain: [word: Word] }>()
const search = ref(''),
  tab = ref('list'),
  error = ref(''),
  now = ref(Date.now())
const clock = setInterval(() => {
  now.value = Date.now()
}, 30000)
onUnmounted(() => clearInterval(clock))
const words = computed(() =>
  props.w.state.words.filter(
    (w) =>
      (tab.value === 'trash'
        ? w.learning?.entry.deletedAt != null
        : w.learning?.entry.deletedAt == null) &&
      [w.text, w.meaning, w.note, ...w.tags]
        .join('\n')
        .toLocaleLowerCase()
        .includes(search.value.toLocaleLowerCase()),
  ),
)
const page = ref(1)
const pageCount = computed(() => Math.max(1, Math.ceil(words.value.length / 50)))
const visibleWords = computed(() => words.value.slice((page.value - 1) * 50, page.value * 50))
watch([search, tab], () => {
  page.value = 1
})
watch(pageCount, (count) => {
  page.value = Math.min(page.value, count)
})
const due = computed(() => dueCards(props.w.state.words, now.value))
function showReview() {
  tab.value = 'review'
}
async function enroll(word: Word, direction: Direction, suspended = false) {
  if (!props.w.canEdit) return
  error.value = ''
  try {
    await props.w.enroll(word.id, direction, suspended)
    now.value = Date.now()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
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
    <LearningExchange :w="w" />
    <p v-if="error" class="web-error" role="alert">{{ error }}</p>
    <div class="words-toolbar">
      <div class="segmented">
        <button :class="{ active: tab === 'list' }" @click="tab = 'list'">全部词句</button
        ><button :class="{ active: tab === 'review' }" @click="showReview">
          复习 · {{ due.length }}</button
        ><button :class="{ active: tab === 'trash' }" @click="tab = 'trash'">
          回收站 · {{ w.state.words.filter((w) => w.learning?.entry.deletedAt != null).length }}
        </button>
      </div>
      <input
        v-if="tab !== 'review'"
        v-model="search"
        aria-label="搜索词句"
        placeholder="搜索词句、释义、注释或标签"
      />
    </div>
    <ReviewPanel v-if="tab === 'review'" :w="w" />
    <template v-else
      ><nav v-if="pageCount > 1" class="message-actions" aria-label="词句分页">
        <button :disabled="page === 1" @click="page--">上一页</button>
        <span>第 {{ page }} / {{ pageCount }} 页 · {{ words.length }} 个词句</span>
        <button :disabled="page === pageCount" @click="page++">下一页</button>
      </nav>
      <div class="word-grid">
        <article v-for="word in visibleWords" :key="word.id" class="word-card">
          <span class="eyebrow">{{ word.language }}</span>
          <h2>{{ word.text }}</h2>
          <p>{{ word.meaning || '等待你的理解与注释。' }}</p>
          <p v-if="word.note" class="word-note">{{ word.note }}</p>
          <div class="tags">
            <span v-for="tag in word.tags" :key="tag">{{ tag }}</span>
          </div>
          <blockquote v-if="word.source">
            {{ word.source.text
            }}<small>{{ word.source.provider }} · {{ word.source.model }}</small>
          </blockquote>
          <div class="message-actions">
            <button :disabled="!w.canEdit" @click="emit('edit', word)">编辑</button
            ><button v-if="word.source" @click="emit('source', word)">查看来源</button
            ><button :disabled="!w.ready('tutor')" @click="emit('explain', word)">
              请助手解释
            </button>
            <template v-if="tab !== 'trash'">
              <button
                v-for="direction in ['recognition', 'production'] as const"
                :key="direction"
                :disabled="!w.canEdit || !word.meaning.trim()"
                @click="
                  enroll(
                    word,
                    direction,
                    wordCards(word).some((c) => c.direction === direction && !c.suspended),
                  )
                "
              >
                {{
                  wordCards(word).some((c) => c.direction === direction && !c.suspended)
                    ? '暂停'
                    : '加入'
                }}{{ direction === 'production' ? '表达' : '识义' }}复习
              </button>
              <small v-if="!word.meaning.trim()">补充释义后可加入复习</small>
            </template>
            <button
              v-else
              :disabled="!w.canEdit"
              @click="w.restoreWord(word.id).catch((e) => (error = String(e)))"
            >
              恢复词句
            </button>
          </div>
        </article>
        <div v-if="!words.length" class="empty-chat">
          <span>词</span>
          <h2>{{ search ? '还没有匹配的词句。' : '让每一次对话留下收获。' }}</h2>
          <p>选中对话里的词句收藏，添加你自己的理解。</p>
        </div>
      </div></template
    >
  </section>
</template>
