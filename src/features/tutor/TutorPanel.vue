<script setup lang="ts">
import { computed, ref, toRef } from 'vue'
import AppIcon from '../../shared/AppIcon.vue'
import MessageList from '../chat/MessageList.vue'
import ImportedContext from '../chat/ImportedContext.vue'
import { useChatStore } from '../chat/store'
import { tutorPlaceholders } from '../../shared/tutor-placeholders'
const chat = useChatStore()
const lane = chat.lanes.tutor
import type { IconName } from '../../shared/AppIcon.vue'

const draft = toRef(lane, 'draft')
const input = ref<HTMLTextAreaElement>()
const activeMode = toRef(chat, 'tutorMode')
const modes: {
  id: string
  label: string
  icon: IconName
  description: string
  placeholder: string
  examples: string[]
}[] = [
  {
    id: 'express',
    label: '怎么说',
    icon: 'chat',
    description: '先用母语说出想法，一起找到自然的表达。',
    placeholder: tutorPlaceholders.express,
    examples: ['“我最近迷上了……”怎么说？', '怎样礼貌地表达不同意见？'],
  },
  {
    id: 'explain',
    label: '解释',
    icon: 'quote',
    description: '从词义到句子结构，把不确定的地方弄明白。',
    placeholder: tutorPlaceholders.explain,
    examples: ['这个词在不同语境中有什么区别？', '帮我拆解这句话的语法结构。'],
  },
  {
    id: 'translate',
    label: '翻译',
    icon: 'translate',
    description: '用母语理解句意，也看看更地道的说法。',
    placeholder: tutorPlaceholders.translate,
    examples: ['帮我翻译这句话，并解释语气。', '这句话直译和自然表达有什么区别？'],
  },
]
const mode = computed(() => modes.find((item) => item.id === activeMode.value) ?? modes[0]!)
function fillDraft(text: string) {
  draft.value = text
  input.value?.focus()
}
async function send() {
  if (!draft.value.trim() || lane.busy || !chat.isReady('tutor')) return
  const text = draft.value
  await chat.send('tutor', text, activeMode.value)
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
    event.preventDefault()
    void send()
  }
}
</script>

<template>
  <section class="tutor content-panel" aria-labelledby="tutor-title">
    <header class="tutor-heading">
      <span class="assistant-icon"><AppIcon name="sparkles" :size="18" /></span>
      <h2 id="tutor-title">语言助手</h2>
      <button
        class="text-button"
        :disabled="lane.busy || !chat.initialized || chat.navigating"
        @click="chat.reset('tutor')"
      >
        重置
      </button>
    </header>
    <div class="tutor-modes" role="group" aria-label="辅导模式">
      <button
        v-for="item in modes"
        :key="item.id"
        :aria-pressed="activeMode === item.id"
        :class="{ active: activeMode === item.id }"
        @click="activeMode = item.id"
      >
        <AppIcon :name="item.icon" :size="14" />{{ item.label }}
      </button>
    </div>
    <div class="tutor-scroll scroll-region" tabindex="0" aria-label="语言辅导内容">
      <ImportedContext :messages="lane.context" />
      <MessageList
        pane="tutor"
        v-if="lane.messages.length"
        :messages="lane.messages"
        :busy="lane.busy"
      />
      <div v-if="!lane.messages.length" class="tutor-intro">
        <div class="tutor-illustration" aria-hidden="true"><span>A</span><span>文</span></div>
        <h3>卡住了？从这里继续。</h3>
        <p>{{ mode.description }}</p>
      </div>
      <div v-if="!lane.messages.length" class="tutor-suggestions">
        <p class="section-label">你可以这样问</p>
        <button v-for="example in mode.examples" :key="example" @click="fillDraft(example)">
          <span>{{ example }}</span
          ><AppIcon name="arrow-right" :size="14" />
        </button>
      </div>
      <div v-if="!lane.messages.length && !chat.terminalContext" class="context-tip">
        <AppIcon name="quote" :size="16" />
        <p>读到不懂的词句？<br /><span>选中对话中的词句，即可解释、翻译或收藏。</span></p>
      </div>
    </div>
    <div class="tutor-composer-dock">
      <div class="composer tutor-composer">
        <label class="sr-only" for="tutor-input">语言方面的问题草稿</label
        ><textarea
          id="tutor-input"
          ref="input"
          v-model="draft"
          :disabled="!chat.initialized || chat.closing"
          @keydown="onKeydown"
          dir="auto"
          rows="3"
          :placeholder="mode.placeholder"
          aria-describedby="tutor-status"
        />
        <div class="composer-actions">
          <span>用母语问，也没关系</span
          ><button
            class="send-button"
            :disabled="!lane.busy && (!chat.isReady('tutor') || !draft.trim() || !chat.tutorModel)"
            :aria-label="lane.busy ? '停止辅导回复' : '发送辅导问题'"
            @click="lane.busy ? chat.stop('tutor') : send()"
          >
            <AppIcon :name="lane.busy ? 'close' : 'arrow-up'" :size="18" />
          </button>
        </div>
      </div>
      <p v-if="lane.error" class="inline-error" role="alert">{{ lane.error }}</p>
      <p id="tutor-status" class="composer-caption">
        {{ chat.isReady('tutor') ? chat.tutorModel : chat.paneLabel('tutor') }}
      </p>
    </div>
  </section>
</template>
