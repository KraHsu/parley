<script setup lang="ts">
import { ref } from 'vue'
import { apiPresets, type ApiKind } from './types'
import type { WebWorkspace } from './store'
const props = defineProps<{ w: WebWorkspace }>()
const dialog = ref<HTMLDialogElement>(),
  preset = ref(0),
  name = ref('OpenAI API'),
  url = ref(apiPresets[0]!.config.endpoint),
  key = ref(''),
  error = ref(''),
  saving = ref(false)
function choose() {
  const p = apiPresets[preset.value]!.config
  name.value = p.name
  url.value = p.endpoint
  key.value = ''
}
async function save() {
  if (saving.value) return
  saving.value = true
  try {
    const p = apiPresets[preset.value]!.config
    props.w.saveProfile(
      {
        id: crypto.randomUUID(),
        name: name.value,
        endpoint: url.value,
        kind: p.kind as ApiKind,
        provider: p.provider,
      },
      key.value,
    )
    if (!(await props.w.persist())) throw new Error(props.w.storageError)
    key.value = ''
    error.value = ''
    props.w.notice = '服务已添加。请为两个面板分别选择模型。'
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    saving.value = false
  }
}
defineExpose({ show: () => dialog.value?.showModal() })
</script>
<template>
  <dialog ref="dialog" class="web-dialog settings-dialog" aria-labelledby="web-settings-title">
    <header>
      <div>
        <span class="eyebrow">MAKE IT YOURS</span>
        <h2 id="web-settings-title">语言与 API 设置</h2>
      </div>
      <button aria-label="关闭设置" @click="dialog?.close()">×</button>
    </header>
    <div class="dialog-body">
      <div class="two-fields">
        <label
          >目标语言<input
            v-model="w.state.settings.target"
            :disabled="!w.canEdit"
            maxlength="100"
            placeholder="en / ja / fr" /></label
        ><label
          >母语<input
            v-model="w.state.settings.native"
            :disabled="!w.canEdit"
            maxlength="100"
            placeholder="zh-CN"
        /></label>
      </div>
      <p class="help">
        语言变更用于新对话；已有消息的对话保留原语言。Web 端只连接 API，使用你所选服务商的 API
        额度。
      </p>
      <section class="privacy-note">
        <strong>密钥只保留在当前标签页内存</strong>
        <p>
          刷新或关闭页面后需要重新填写。密钥不会写入本地学习数据或备份，也不会发送给
          GitHub。浏览器会直接访问你填写的 API 地址，该服务须允许浏览器跨域请求。
        </p>
      </section>
      <section v-for="profile in w.state.profiles" :key="profile.id" class="saved-profile">
        <strong>{{ profile.name }}</strong
        ><small>{{ profile.endpoint }}</small
        ><label
          >本次会话 API Key<input
            type="password"
            :value="w.keys.get(profile.id) ?? ''"
            :disabled="!w.canEdit || w.busy.main || w.busy.tutor"
            autocomplete="off"
            :aria-label="`${profile.name} API Key`"
            @input="w.keys.set(profile.id, ($event.target as HTMLInputElement).value.trim())"
        /></label>
        <div class="message-actions">
          <button
            :disabled="!w.keys.get(profile.id) || w.checking.has(profile.id)"
            @click="w.check(profile)"
          >
            {{ w.checking.has(profile.id) ? '读取中…' : '读取模型列表' }}</button
          ><button :disabled="w.busy.main || w.busy.tutor" @click="w.keys.delete(profile.id)">
            清除密钥</button
          ><button
            :disabled="!w.canEdit || w.busy.main || w.busy.tutor"
            @click="w.removeProfile(profile.id)"
          >
            删除闲置配置
          </button>
        </div>
        <p v-if="w.models.get(profile.id)?.length" class="help">
          已读取 {{ w.models.get(profile.id)?.length }} 个模型，在对话面板中选择。
        </p>
      </section>
      <form :inert="saving" class="new-profile" @submit.prevent="save">
        <h3>添加 API 服务</h3>
        <label
          >服务商<select v-model="preset" :disabled="!w.canEdit" @change="choose">
            <option v-for="(p, index) in apiPresets" :key="p.label" :value="index">
              {{ p.label }}
            </option>
          </select></label
        ><label>配置名称<input v-model="name" required maxlength="100" /></label
        ><label
          >API 服务地址<input
            v-model="url"
            required
            type="url"
            maxlength="2048"
            placeholder="https://example.com/v1" /></label
        ><label>API Key<input v-model="key" type="password" autocomplete="off" /></label>
        <p v-if="error" class="web-error" role="alert">{{ error }}</p>
        <button class="primary" :disabled="!w.canEdit || saving">保存服务</button>
      </form>
      <p v-if="w.error" class="web-error" role="alert">{{ w.error }}</p>
      <p class="help">
        模型列表和浏览器直连能力因服务商而异；读取失败时可手动填写模型
        ID。需要调整地址时添加新配置，避免旧历史意外发往另一个服务。
      </p>
    </div>
    <footer><button class="primary" @click="dialog?.close()">完成</button></footer>
  </dialog>
</template>
