<script setup lang="ts">
import { useVocabularyStore } from './store'
const vocabulary = useVocabularyStore()
</script>
<template>
  <div class="word-list" aria-label="收藏列表" :aria-busy="vocabulary.loading">
    <button
      v-for="entry in vocabulary.entries"
      :key="entry.id"
      class="word-list-item"
      @click="vocabulary.open(entry.id)"
    >
      <span class="word-list-heading"
        ><strong dir="auto">{{ entry.text }}</strong
        ><small>{{ entry.language }}</small></span
      >
      <span dir="auto" class="word-list-meaning">{{ entry.meaning || '待补充释义' }}</span
      ><small>{{ new Date(entry.updatedAt).toLocaleDateString() }}</small>
    </button>
    <div
      v-if="!vocabulary.entries.length && !vocabulary.loading && !vocabulary.error"
      class="vocabulary-welcome"
    >
      <p class="overline">WORDS WORTH KEEPING</p>
      <h2>
        {{
          vocabulary.query.trash
            ? '回收站是空的'
            : vocabulary.query.search
              ? '没有匹配的词句'
              : '把喜欢的表达，留给未来的你。'
        }}
      </h2>
      <p>在对话或同步的终端内容中选中词句，连同原句和自己的理解一起保存。</p>
      <button
        v-if="!vocabulary.query.trash"
        class="secondary-button"
        :disabled="!vocabulary.initialized"
        @click="vocabulary.start()"
      >
        添加第一条词句
      </button>
    </div>
    <nav v-if="vocabulary.total > 50" class="word-pagination" aria-label="词句分页">
      <button
        class="secondary-button"
        :disabled="!vocabulary.query.offset || vocabulary.loading"
        @click="vocabulary.page(vocabulary.query.offset - 50)"
      >
        上一页</button
      ><span
        >{{ vocabulary.query.offset + 1 }}–{{
          Math.min(vocabulary.query.offset + 50, vocabulary.total)
        }}
        / {{ vocabulary.total }}</span
      ><button
        class="secondary-button"
        :disabled="vocabulary.query.offset + 50 >= vocabulary.total || vocabulary.loading"
        @click="vocabulary.page(vocabulary.query.offset + 50)"
      >
        下一页
      </button>
    </nav>
  </div>
</template>
