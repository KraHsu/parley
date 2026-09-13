<script setup lang="ts">
import { ref } from 'vue'
import AppIcon from '../../shared/AppIcon.vue'
const search = ref('')
</script>

<template>
  <section class="vocabulary content-panel" aria-labelledby="vocabulary-title">
    <header class="panel-toolbar">
      <h1 id="vocabulary-title">词句收藏 <span class="count-badge">0</span></h1>
      <span class="mode-label">你的表达积累</span>
    </header>
    <div class="vocabulary-search">
      <AppIcon name="search" :size="17" /><label class="sr-only" for="vocabulary-search"
        >搜索收藏的词句</label
      ><input
        id="vocabulary-search"
        v-model="search"
        type="search"
        placeholder="搜索词语、释义或注释…"
      />
    </div>
    <div class="vocabulary-scroll scroll-region" tabindex="0" aria-label="收藏词句列表">
      <div class="vocabulary-welcome">
        <span class="vocabulary-mark"><AppIcon name="book" :size="30" /></span>
        <p class="overline">WORDS WORTH KEEPING</p>
        <h2>{{ search ? '还没有匹配的词句' : '把喜欢的表达，留给未来的你。' }}</h2>
        <p>
          {{
            search
              ? `目前没有包含“${search}”的收藏。`
              : '未来可以从对话中收藏词句，连同原句和自己的理解一起保存。'
          }}
        </p>
        <span class="coming-soon">收藏与注释功能正在准备中</span>
      </div>
    </div>
  </section>
</template>
