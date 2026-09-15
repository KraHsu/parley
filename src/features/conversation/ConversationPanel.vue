<script setup lang="ts">
import { ref, toRef } from 'vue'
import { useSettingsStore } from '../settings/store'
import AppIcon from '../../shared/AppIcon.vue'
import MessageList from '../chat/MessageList.vue'
import ImportedContext from '../chat/ImportedContext.vue'
import { useChatStore } from '../chat/store'
const chat = useChatStore()
const lane = chat.lanes.main
import type { IconName } from '../../shared/AppIcon.vue'

const settings = useSettingsStore()
const draft = toRef(lane, 'draft')
const input = ref<HTMLTextAreaElement>()
defineEmits<{ connect: [] }>()
const topics: { title: string; description: string; icon: IconName; tone: string }[] = [
  { title: '生活里的小事', description: '从今天的一个瞬间聊起', icon: 'coffee', tone: 'peach' },
  { title: '去看看世界', description: '下一站，你想去哪里？', icon: 'compass', tone: 'blue' },
  { title: '分享你的热爱', description: '一本书、一部电影或新爱好', icon: 'sun', tone: 'yellow' },
  { title: '一点新的想法', description: '让好奇心带着对话往前走', icon: 'leaf', tone: 'green' },
]
const prompts: Record<string, string[]> = {
  en: [
    'I’d like to tell you about my day.',
    'If you could travel anywhere, where would you go?',
    'I’d like to talk about something I really enjoy.',
    'What is something interesting we could explore together?',
  ],
  'zh-CN': [
    '我想和你聊聊我今天的生活。',
    '如果可以去任何地方旅行，你会去哪里？',
    '我想聊聊我特别喜欢的一件事。',
    '有什么有趣的话题值得我们一起探索？',
  ],
  ja: [
    '今日は、私の一日について話したいです。',
    'どこにでも旅行できるなら、どこに行きたいですか？',
    '私の好きなことについて話したいです。',
    '一緒に考えてみたい面白い話題はありますか？',
  ],
  ko: [
    '오늘 하루에 대해 이야기하고 싶어요.',
    '어디든 여행할 수 있다면 어디로 가고 싶어요?',
    '제가 정말 좋아하는 것에 대해 이야기하고 싶어요.',
    '함께 알아볼 만한 흥미로운 주제가 있을까요?',
  ],
  fr: [
    'J’aimerais te raconter ma journée.',
    'Si tu pouvais voyager n’importe où, où irais-tu ?',
    'J’aimerais parler de quelque chose que j’aime beaucoup.',
    'Quel sujet intéressant pourrions-nous explorer ensemble ?',
  ],
  de: [
    'Ich würde dir gern von meinem Tag erzählen.',
    'Wenn du überallhin reisen könntest, wohin würdest du gehen?',
    'Ich möchte über etwas sprechen, das mir viel Freude macht.',
    'Welches interessante Thema könnten wir gemeinsam erkunden?',
  ],
  es: [
    'Me gustaría contarte cómo ha sido mi día.',
    'Si pudieras viajar a cualquier lugar, ¿adónde irías?',
    'Me gustaría hablar de algo que disfruto mucho.',
    '¿Qué tema interesante podríamos explorar juntos?',
  ],
  ar: [
    'أود أن أحدثك عن يومي.',
    'لو استطعت السفر إلى أي مكان، فأين ستذهب؟',
    'أود أن أتحدث عن شيء أحبه كثيرًا.',
    'ما الموضوع الممتع الذي يمكننا استكشافه معًا؟',
  ],
}
function chooseTopic(index: number) {
  draft.value = prompts[settings.targetLanguage]?.[index] ?? ''
  input.value?.focus()
}
async function send() {
  if (!draft.value.trim() || lane.busy || !chat.isReady('main')) return
  const text = draft.value
  await chat.send('main', text)
}
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
    event.preventDefault()
    void send()
  }
}
</script>

<template>
  <section class="conversation content-panel" aria-labelledby="conversation-title">
    <header class="panel-toolbar">
      <h1 id="conversation-title">
        {{ lane.messages.length ? '对话练习' : '新的对话' }}
        <span class="subtle-label">{{ settings.targetLanguageLabel }}</span>
      </h1>
      <button
        class="text-button"
        :disabled="lane.busy || !chat.initialized || chat.navigating"
        @click="chat.reset('main')"
      >
        新对话
      </button>
    </header>

    <div class="conversation-scroll scroll-region" tabindex="0" aria-label="对话消息">
      <ImportedContext :messages="lane.context" />
      <MessageList
        pane="main"
        v-if="lane.messages.length"
        :messages="lane.messages"
        :busy="lane.busy"
      />
      <div v-else class="conversation-welcome">
        <div class="welcome-mark" aria-hidden="true">
          <AppIcon name="chat" :size="30" /><span><AppIcon name="sparkles" :size="16" /></span>
        </div>
        <p class="overline">A LITTLE CONVERSATION, EVERY DAY</p>
        <h2>今天，想聊些什么？</h2>
        <p class="welcome-description">
          用
          <strong>{{ settings.targetLanguageLabel }}</strong>
          分享你的世界。<br />不确定怎么表达？语言助手就在你身边。
        </p>
        <div class="topic-grid">
          <button
            v-for="(topic, index) in topics"
            :key="topic.title"
            class="topic-card"
            @click="chooseTopic(index)"
          >
            <span class="topic-icon" :class="topic.tone"
              ><AppIcon :name="topic.icon" :size="20"
            /></span>
            <span class="topic-copy"
              ><strong>{{ topic.title }}</strong
              ><span>{{ topic.description }}</span></span
            >
            <AppIcon name="arrow-right" :size="15" class="topic-arrow" />
          </button>
        </div>
        <p class="welcome-hint">选一个话题，或直接写下你想说的话。</p>
      </div>
    </div>

    <div class="composer-dock">
      <div class="composer main-composer">
        <label class="sr-only" for="conversation-input">目标语言消息草稿</label>
        <textarea
          id="conversation-input"
          ref="input"
          v-model="draft"
          :disabled="!chat.initialized || chat.closing"
          @keydown="onKeydown"
          dir="auto"
          rows="2"
          :placeholder="`试着用 ${settings.targetLanguageLabel} 表达你的想法…`"
          aria-describedby="conversation-status"
        />
        <div class="composer-actions">
          <span class="composer-language"
            ><AppIcon name="globe" :size="14" />{{ settings.targetLanguageLabel }}</span
          >
          <button
            class="send-button"
            :disabled="!lane.busy && (!chat.isReady('main') || !draft.trim() || !chat.mainModel)"
            :aria-label="lane.busy ? '停止回复' : '发送消息'"
            @click="lane.busy ? chat.stop('main') : send()"
          >
            <AppIcon :name="lane.busy ? 'close' : 'arrow-up'" :size="19" />
          </button>
        </div>
      </div>
      <p v-if="lane.error" class="inline-error" role="alert">{{ lane.error }}</p>
      <p id="conversation-status" class="composer-caption">
        <span class="tiny-dot" />{{ chat.isReady('main') ? chat.mainModel : chat.paneLabel('main')
        }}<button @click="$emit('connect')">
          连接设置<AppIcon name="arrow-right" :size="12" />
        </button>
      </p>
    </div>
  </section>
</template>
