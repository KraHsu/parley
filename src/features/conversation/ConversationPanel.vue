<script setup lang="ts">
import { useSettingsStore } from '../settings/store'

const settings = useSettingsStore()
</script>

<template>
  <section class="panel conversation" aria-labelledby="conversation-title">
    <header class="panel-heading">
      <div>
        <p class="eyebrow">CONVERSATION</p>
        <h2 id="conversation-title">用语言，探索你关心的事</h2>
      </div>
      <span class="tag">{{ settings.targetLanguageLabel }}</span>
    </header>

    <div class="conversation-empty">
      <span class="empty-symbol" aria-hidden="true">Aa</span>
      <h3>从一个真正想聊的话题开始。</h3>
      <p>
        聊今天的生活、一个新想法，或者正在读的书。<br />遇到不会表达的地方，右边的语言助手会帮助你。
      </p>
      <div class="example-card">
        <span class="eyebrow">话题灵感</span>
        <p>最近学到的一件有趣的事</p>
        <p>想去旅行的地方，以及原因</p>
        <p>向朋友推荐一本书或一部电影</p>
      </div>
    </div>

    <div class="composer">
      <label class="sr-only" for="conversation-input">目标语言消息</label>
      <textarea
        id="conversation-input"
        disabled
        rows="2"
        :placeholder="`在这里用 ${settings.targetLanguageLabel} 表达你的想法…`"
        aria-describedby="conversation-status"
      />
      <div class="composer-footer">
        <span id="conversation-status">对话功能将在 Codex 接入后开放</span>
        <button class="button button-primary" disabled>发送</button>
      </div>
    </div>
  </section>
</template>
