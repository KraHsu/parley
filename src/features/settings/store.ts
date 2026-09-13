import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { LanguageOption } from '../../shared/types'

export const languages: LanguageOption[] = [
  { code: 'en', label: 'English' },
  { code: 'zh-CN', label: '简体中文' },
  { code: 'ja', label: '日本語' },
  { code: 'ko', label: '한국어' },
  { code: 'fr', label: 'Français' },
  { code: 'de', label: 'Deutsch' },
  { code: 'es', label: 'Español' },
  { code: 'ar', label: 'العربية' },
]

// M0 keeps preferences in memory. M3 will persist them through the Rust storage layer.
export const useSettingsStore = defineStore('settings', () => {
  const nativeLanguage = ref('zh-CN')
  const targetLanguage = ref('en')
  const targetLanguageLabel = computed(
    () => languages.find((language) => language.code === targetLanguage.value)?.label ?? '目标语言',
  )

  return { nativeLanguage, targetLanguage, targetLanguageLabel }
})
