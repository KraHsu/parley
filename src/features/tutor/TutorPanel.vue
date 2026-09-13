<script setup lang="ts">
const actions = ['这句话怎么说？', '解释选中的词句', '翻译并讲解语法']
</script>

<template>
  <section class="panel tutor" aria-labelledby="tutor-title">
    <header class="panel-heading">
      <div>
        <p class="eyebrow">LANGUAGE COMPANION</p>
        <h2 id="tutor-title">语言助手</h2>
      </div>
      <span class="status-dot" aria-label="尚未连接" />
    </header>
    <p class="muted">可以用母语提问。先弄懂，再回到对话。</p>
    <div class="tutor-actions">
      <button v-for="action in actions" :key="action" class="action-button" disabled>
        {{ action }} <span aria-hidden="true">↗</span>
      </button>
    </div>
    <div class="tutor-note">
      <p>一个随时可以求助的角落</p>
      <span>表达提示、词义和语法解释都会出现在这里。首版将按需调用独立辅导会话。</span>
    </div>
    <label class="sr-only" for="tutor-input">语言方面的问题</label>
    <textarea id="tutor-input" disabled rows="3" placeholder="我想表达……，用目标语言怎么说？" />
    <p class="footnote">辅导模型待连接后选择 · 同样使用你的 Codex 额度</p>
  </section>
</template>
