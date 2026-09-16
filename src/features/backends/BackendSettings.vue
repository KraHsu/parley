<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import ConnectionSettings from '../codex/ConnectionSettings.vue'
import { useChatStore } from '../chat/store'
import { useBackendStore } from './store'
import { supportsManagedTurns, presets, type BackendProfile, type ProfileConfig } from './types'
import { isDesktop } from '../../shared/desktop'

defineProps<{ tutorOnly?: boolean }>()
const chat = useChatStore()
const backends = useBackendStore()
const root = ref<HTMLElement>()
const preset = ref(0)
const editing = ref<BackendProfile>()
const config = ref<ProfileConfig>({ ...presets[0]!.config })
const key = ref('')
const persist = ref(true)
const busy = ref(false)
const error = ref('')
const notice = ref('')
const showingForm = ref(false)
const switching = ref<{
  pane: 'main' | 'tutor'
  profileId: string
  revision: number
  sourceId: string
}>()
const available = computed(() =>
  backends.profiles.filter(
    (p) => p.config.enabled && (p.config.kind === 'codex' || supportsManagedTurns(p.config.kind)),
  ),
)
const editable = computed(() => backends.profiles.filter((p) => p.id !== 'codex-default'))
const clearKey = () => {
  key.value = ''
  switching.value = undefined
}
function cancelForm() {
  showingForm.value = false
  clearKey()
}
function staleBinding(pane: 'main' | 'tutor') {
  const binding = chat.lanes[pane].backend
  return backends.profiles.some(
    (p) => p.id === binding.profileId && p.revision !== binding.profileRevision,
  )
}
let dialog: HTMLDialogElement | null | undefined
onMounted(() => {
  void backends.load()
  dialog = root.value?.closest('dialog')
  dialog?.addEventListener('close', clearKey)
})
onUnmounted(() => dialog?.removeEventListener('close', clearKey))
function choosePreset() {
  config.value = { ...presets[preset.value]!.config }
  clearKey()
}
function edit(profile?: BackendProfile) {
  editing.value = profile
  config.value = { ...(profile?.config ?? presets[preset.value]!.config) }
  error.value = ''
  notice.value = ''
  clearKey()
  showingForm.value = true
}
async function save() {
  if (busy.value || !isDesktop()) return
  busy.value = true
  error.value = ''
  notice.value = ''
  const secret = key.value
  clearKey()
  try {
    const profile = await backends.save({ ...config.value }, editing.value)
    editing.value = profile
    if (secret && profile.config.kind !== 'codex')
      await backends.setCredential(profile, secret, persist.value)
    notice.value =
      profile.config.kind === 'codex'
        ? '配置已保存。连接此 Codex 后，可为主聊或语言助手选择服务与模型。'
        : '配置已保存。为主聊或语言助手选择此服务后，可读取或手动填写模型 ID。'
    showingForm.value = false
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
async function removeKey(profile: BackendProfile) {
  error.value = ''
  try {
    await backends.removeCredential(profile)
    notice.value = '已删除此服务的 API 密钥，本地对话和词句保留。'
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}
async function select(pane: 'main' | 'tutor', event: Event) {
  const profileId = (event.target as HTMLSelectElement).value
  const lane = chat.lanes[pane]
  ;(event.target as HTMLSelectElement).value = lane.backend.profileId
  const profile = backends.profiles.find((p) => p.id === profileId)
  if (
    !profile ||
    (profileId === lane.backend.profileId && profile.revision === lane.backend.profileRevision)
  )
    return
  if (lane.context.length || lane.messages.some((m) => m.text.trim())) {
    switching.value = { pane, profileId, revision: profile.revision, sourceId: lane.id }
  } else await chat.selectBackend(pane, profileId)
}
async function switchConversation(carry: boolean) {
  const choice = switching.value
  if (!choice || chat.navigating) return
  if (
    chat.lanes[choice.pane].id !== choice.sourceId ||
    backends.profiles.find((p) => p.id === choice.profileId)?.revision !== choice.revision
  ) {
    error.value = '会话或服务配置已改变，请重新选择。'
    switching.value = undefined
    return
  }
  await chat.selectBackend(choice.pane, choice.profileId, carry)
  if (
    chat.lanes[choice.pane].backend.profileId === choice.profileId &&
    chat.lanes[choice.pane].id !== choice.sourceId
  )
    switching.value = undefined
  else error.value = chat.lanes[choice.pane].error
}
function profileLabel(id: string) {
  return backends.profiles.find((p) => p.id === id)?.config.name ?? '已归档的服务'
}
function modelChoices(pane: 'main' | 'tutor') {
  const id = chat.lanes[pane].backend.profileId
  return id === 'codex-default' ? chat.models.map((m) => m.model) : backends.state(id).models
}
</script>

<template>
  <div ref="root">
    <section class="settings-section">
      <h3>模型服务</h3>
      <p class="settings-help">
        主聊与语言助手可以使用不同服务。API 使用你的服务商 API 额度；Codex 使用本机官方登录。Claude
        Code GUI 使用 API Key。连接检查读取模型列表及 CLI 能力，不生成回答。
      </p>
      <p v-if="!isDesktop()" class="settings-help">
        浏览器仅预览界面，请在桌面应用中配置模型服务。
      </p>
      <template
        v-for="pane in (tutorOnly ? ['tutor'] : ['main', 'tutor']) as ('main' | 'tutor')[]"
        :key="pane"
      >
        <label class="settings-field">
          {{ pane === 'main' ? '主聊服务' : '语言助手服务' }}
          <select
            :value="chat.lanes[pane].backend.profileId"
            :disabled="!chat.initialized || chat.lanes[pane].busy || chat.navigating || !!switching"
            @change="select(pane, $event)"
          >
            <option
              v-if="!available.some((p) => p.id === chat.lanes[pane].backend.profileId)"
              :value="chat.lanes[pane].backend.profileId"
              disabled
            >
              {{ profileLabel(chat.lanes[pane].backend.profileId) }}（不可用）
            </option>
            <option v-for="profile in available" :key="profile.id" :value="profile.id">
              {{ profile.config.name }}
            </option>
          </select>
        </label>
        <section
          v-if="switching && switching.pane === pane"
          class="settings-section"
          aria-label="选择新会话的历史"
        >
          <p>
            将{{ switching.pane === 'main' ? '主聊' : '语言助手' }}切换到
            {{ profileLabel(switching.profileId) }}。
          </p>
          <p class="settings-help">
            可带入当前可见的文字历史，首次提问时发送给新服务。旧会话仍保留；中断内容带有状态标记。最多
            200 条、24 KiB，超过时请选择空白会话。
          </p>
          <div class="settings-actions">
            <button
              class="secondary-button"
              :disabled="chat.navigating"
              @click="switchConversation(false)"
            >
              空白新会话
            </button>
            <button
              class="primary-button"
              :disabled="chat.navigating"
              @click="switchConversation(true)"
            >
              带入可见历史
            </button>
            <button class="text-button" :disabled="chat.navigating" @click="switching = undefined">
              取消
            </button>
          </div>
        </section>
        <label v-if="chat.lanes[pane].backend.kind !== 'codex'" class="settings-field">
          {{ pane === 'main' ? '主聊模型 ID' : '语言助手模型 ID' }}
          <input
            :value="pane === 'main' ? chat.mainModel : chat.tutorModel"
            :list="`models-${pane}`"
            :disabled="chat.lanes[pane].busy || chat.navigating"
            placeholder="读取模型列表，或填写服务商提供的模型 ID"
            autocomplete="off"
            spellcheck="false"
            @input="
              pane === 'main'
                ? (chat.mainModel = ($event.target as HTMLInputElement).value)
                : (chat.tutorModel = ($event.target as HTMLInputElement).value)
            "
          />
          <datalist :id="`models-${pane}`">
            <option v-for="model in modelChoices(pane)" :key="model" :value="model" />
          </datalist>
        </label>
        <p
          v-if="
            chat.lanes[pane].context.length || chat.lanes[pane].messages.some((m) => m.text.trim())
          "
          class="settings-help"
        >
          <button
            class="secondary-button"
            :disabled="!!switching || chat.lanes[pane].busy || chat.navigating"
            @click="chat.selectBackend(pane, chat.lanes[pane].backend.profileId, true)"
          >
            {{ pane === 'main' ? '主聊' : '助手' }}：带入历史并使用当前模型
          </button>
          创建新会话，历史在下一次提问时发送。
        </p>
        <p v-if="staleBinding(pane)" class="settings-help">
          此会话使用旧配置。
          <button
            class="secondary-button"
            :disabled="chat.lanes[pane].busy || chat.navigating"
            @click="chat.selectBackend(pane, chat.lanes[pane].backend.profileId)"
          >
            使用新配置开始对话
          </button>
        </p>
      </template>

      <p class="settings-help">
        更换服务会开启新会话，旧记录保留。未提供余额查询的 API 不显示剩余额度。
      </p>
      <div v-for="profile in editable" :key="profile.id" class="backend-profile">
        <template v-if="profile.config.kind === 'codex'">
          <ConnectionSettings :profile="profile" :tutor-only="tutorOnly" />
          <button class="secondary-button" :disabled="busy" @click="edit(profile)">
            编辑 Codex 配置
          </button>
        </template>
        <template v-else>
          <strong>{{ profile.config.name }}</strong>
          <span class="subtle-label">{{
            !profile.config.enabled
              ? '已停用'
              : backends.state(profile.id).credential.persistence === 'system'
                ? '密钥保存在系统凭据库'
                : backends.state(profile.id).credential.persistence === 'session'
                  ? '密钥仅本次会话可用'
                  : '待配置密钥'
          }}</span>
          <p class="settings-help">{{ profile.config.endpoint || profile.config.binaryPath }}</p>
          <div class="connection-actions">
            <button class="secondary-button" :disabled="busy" @click="edit(profile)">
              编辑 / 更换密钥
            </button>
            <button
              class="secondary-button"
              :disabled="
                !backends.state(profile.id).credential.configured ||
                backends.state(profile.id).checking
              "
              @click="backends.check(profile)"
            >
              {{ backends.state(profile.id).checking ? '读取中…' : '检查连接 / 读取模型' }}
            </button>
            <button
              v-if="backends.state(profile.id).credential.configured"
              class="secondary-button"
              @click="removeKey(profile)"
            >
              删除密钥
            </button>
          </div>
          <p v-if="backends.state(profile.id).models.length" class="settings-help">
            已读取 {{ backends.state(profile.id).models.length }} 个模型。
          </p>
          <p v-if="backends.state(profile.id).error" class="inline-error" role="alert">
            {{ backends.state(profile.id).error }}
          </p>
        </template>
      </div>
      <button class="secondary-button" :disabled="!isDesktop() || busy" @click="edit()">
        添加模型服务
      </button>
      <form v-if="showingForm" class="backend-form" @submit.prevent="save">
        <label v-if="!editing" class="settings-field"
          >服务商<select v-model="preset" :disabled="busy" @change="choosePreset">
            <option
              v-for="(item, index) in presets"
              :key="item.label"
              :value="index"
              :disabled="!item.available"
            >
              {{ item.label }}
            </option>
          </select></label
        >
        <label class="settings-field"
          >配置名称<input v-model="config.name" required maxlength="160" :disabled="busy"
        /></label>
        <label
          v-if="config.kind === 'claude_code' || config.kind === 'codex'"
          class="settings-field"
        >
          {{ config.kind === 'codex' ? 'Codex' : 'Claude Code' }} 可执行文件路径
          <input
            v-model="config.binaryPath"
            required
            :disabled="busy"
            :placeholder="
              config.kind === 'codex'
                ? 'Codex 完整路径；Windows 使用 codex.cmd 或 codex.exe'
                : '本机 claude 的完整路径'
            "
            spellcheck="false"
          />
        </label>
        <p v-if="config.kind === 'claude_code'" class="settings-help">
          使用你安装的官方 Claude Code（已验证 2.1.269）。GUI 使用 API Key
          与独立会话；原生终端保留官方登录。
        </p>
        <label
          v-if="config.kind !== 'claude_code' && config.kind !== 'codex'"
          class="settings-field"
          >API 服务地址<input
            v-model="config.endpoint"
            required
            type="url"
            :disabled="busy"
            placeholder="填写与密钥所属区域对应的 API 基础地址"
            spellcheck="false"
        /></label>
        <p v-if="config.provider === 'qwen'" class="settings-help">
          请从 Model Studio 控制台复制当前区域的 OpenAI 兼容基础地址，通常以 /compatible-mode/v1
          结尾。
        </p>
        <label v-if="config.kind !== 'codex'" class="settings-field"
          >{{ editing ? '新 API Key（留空保留原密钥）' : 'API Key'
          }}<input
            v-model="key"
            type="password"
            autocomplete="off"
            spellcheck="false"
            :disabled="busy"
        /></label>
        <label v-if="config.kind !== 'codex'" class="settings-field"
          >密钥保存方式<select v-model="persist" :disabled="busy">
            <option :value="true">系统凭据库</option>
            <option :value="false">仅本次会话</option>
          </select></label
        >
        <p v-if="config.kind !== 'codex'" class="settings-help">
          重新填写同一密钥可以继续原对话；换成其他密钥时，请新建对话或带入可见历史。
        </p>
        <label
          ><input v-model="config.enabled" type="checkbox" :disabled="busy" /> 启用此服务</label
        >
        <div class="connection-actions">
          <button type="submit" class="primary-button" :disabled="busy">
            {{ busy ? '保存中…' : '保存服务' }}</button
          ><button type="button" class="secondary-button" :disabled="busy" @click="cancelForm">
            取消
          </button>
        </div>
      </form>
      <p v-if="error || backends.error" class="inline-error" role="alert">
        {{ error || backends.error }}
      </p>
      <p v-if="notice" class="settings-help" role="status">{{ notice }}</p>
    </section>
    <ConnectionSettings :tutor-only="tutorOnly" />
  </div>
</template>

<style scoped>
.backend-profile {
  padding: 12px 0;
  border-bottom: 1px solid var(--line, #ddd);
  margin-bottom: 12px;
}
.backend-profile strong,
.backend-profile .subtle-label {
  display: block;
}
.backend-profile p {
  overflow-wrap: anywhere;
}
.backend-form {
  display: grid;
  gap: 12px;
  margin-top: 16px;
}
</style>
