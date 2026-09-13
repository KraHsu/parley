<script setup lang="ts">
import { ref, watch, useId } from 'vue'
import { languages } from './store'
const props = defineProps<{ modelValue: string; disabled?: boolean; label: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const value = ref(props.modelValue)
const error = ref('')
const listId = useId()
watch(
  () => props.modelValue,
  (v) => (value.value = v),
)
function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !event.isComposing) {
    event.preventDefault()
    commit()
  }
}
function commit() {
  const code = value.value.trim()
  try {
    if (!code || code.length > 100) throw Error()
    // Intl handles normal BCP 47 tags; private-use tags also have a defined syntax.
    const canonical = /^x(?:-[a-z0-9]{1,8})+$/i.test(code)
      ? code.toLowerCase()
      : Intl.getCanonicalLocales(code)[0]
    if (!canonical) throw Error()
    emit('update:modelValue', canonical)
    value.value = canonical
    error.value = ''
  } catch {
    error.value = '请输入语言代码，例如 en、pt-BR 或 zh-Hant。'
  }
}
</script>
<template>
  <label class="settings-field language-picker"
    >{{ label
    }}<input
      v-model="value"
      :aria-label="`${label}语言代码`"
      :disabled="disabled"
      :list="listId"
      placeholder="语言代码，如 pt-BR"
      @change="commit"
      @keydown="onKeydown"
    /><datalist :id="listId">
      <option v-for="language in languages" :key="language.code" :value="language.code">
        {{ language.label }}
      </option></datalist
    ><span v-if="error" class="inline-error" role="alert">{{ error }}</span></label
  >
</template>
