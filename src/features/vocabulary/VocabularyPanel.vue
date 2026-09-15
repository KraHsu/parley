<script setup lang="ts">
import { toRef, watch } from 'vue'
import { useVocabularyStore } from './store'
import { useChatStore } from '../chat/store'
import ReviewPanel from './ReviewPanel.vue'
import VocabularyExchange from './VocabularyExchange.vue'
import VocabularyList from './VocabularyList.vue'
import VocabularyDetail from './VocabularyDetail.vue'
defineProps<{ companion?: boolean }>()
const vocabulary = useVocabularyStore()
const chat = useChatStore()
const view = toRef(vocabulary, 'activeTab')
watch(view, (tab) => {
  if (tab === 'words' && vocabulary.selected) void vocabulary.open(vocabulary.selected.id)
})
</script>
<template>
  <section
    :inert="chat.closing"
    class="vocabulary content-panel"
    aria-labelledby="vocabulary-title"
  >
    <header class="panel-toolbar">
      <h1 id="vocabulary-title">
        {{ view === 'review' ? '词句复习' : '词句收藏' }}
        <span v-if="view === 'words'" class="count-badge">{{ vocabulary.total }}</span>
      </h1>
      <button
        class="secondary-button"
        :disabled="!vocabulary.initialized || vocabulary.busy"
        @click="vocabulary.start()"
      >
        添加词句
      </button>
    </header>
    <nav v-if="!companion" class="word-companion-tabs" aria-label="词句学习视图">
      <button class="text-button" :aria-pressed="view === 'words'" @click="view = 'words'">
        词句本</button
      ><button class="text-button" :aria-pressed="view === 'review'" @click="view = 'review'">
        复习
      </button>
    </nav>
    <div v-if="vocabulary.notice" class="word-notice" role="status">{{ vocabulary.notice }}</div>
    <div v-if="vocabulary.error || vocabulary.draftError" class="storage-banner" role="alert">
      {{ vocabulary.error || vocabulary.draftError
      }}<button
        class="text-button"
        @click="vocabulary.initialized ? vocabulary.refresh() : vocabulary.initialize()"
      >
        重试读取
      </button>
    </div>
    <div class="vocabulary-scroll scroll-region" tabindex="0" aria-label="词句学习内容">
      <ReviewPanel v-if="view === 'review'" />
      <VocabularyDetail v-else-if="vocabulary.selected" />
      <template v-else>
        <details v-if="vocabulary.drafts.length" class="word-drafts">
          <summary>{{ vocabulary.drafts.length }} 份未完成草稿</summary>
          <button
            v-for="draft in vocabulary.drafts"
            :key="draft.id"
            class="text-button"
            @click="vocabulary.resume(draft)"
          >
            {{ draft.fields.text || '未命名词句' }} · 继续编辑
          </button>
        </details>
        <div class="word-filters">
          <label class="word-search"
            >搜索<input
              v-model="vocabulary.query.search"
              type="search"
              placeholder="词句、释义或注释…" /></label
          ><label
            >语言<select v-model="vocabulary.query.language">
              <option value="">全部语言</option>
              <option v-for="language in vocabulary.languages" :key="language">
                {{ language }}
              </option>
            </select></label
          ><label
            >类型<select v-model="vocabulary.query.kind">
              <option value="">全部类型</option>
              <option value="word">词</option>
              <option value="phrase">短语</option>
              <option value="sentence">句子</option>
            </select></label
          ><label
            >释义<select v-model="vocabulary.query.hasMeaning">
              <option :value="null">全部</option>
              <option :value="true">已有释义</option>
              <option :value="false">待补充</option>
            </select></label
          ><label
            >排序<select v-model="vocabulary.query.sort">
              <option value="updated">最近更新</option>
              <option value="created">最近收藏</option>
            </select></label
          ><label
            >标签<select v-model="vocabulary.query.tag">
              <option value="">全部标签</option>
              <option v-for="tag in vocabulary.tags" :key="tag">{{ tag }}</option>
            </select></label
          >
          <label
            >复习<select v-model="vocabulary.query.reviewStatus">
              <option value="">全部状态</option>
              <option value="none">未加入</option>
              <option value="due">已到期</option>
              <option value="suspended">有暂停卡片</option>
            </select></label
          >
          <label class="word-checkbox"
            ><input v-model="vocabulary.query.trash" type="checkbox" />回收站</label
          >
        </div>
        <p v-if="vocabulary.loading" class="settings-help" role="status">正在读取词句…</p>
        <VocabularyList />
        <VocabularyExchange />
      </template>
    </div>
  </section>
</template>
