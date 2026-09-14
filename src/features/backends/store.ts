import { reactive, ref } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { isDesktop } from '../../shared/desktop'
import {
  apiSupported,
  type BackendProfile,
  type BackendRuntime,
  type CredentialStatus,
  type ProfileConfig,
} from './types'

const describe = (e: unknown) => (e instanceof Error ? e.message : String(e))
export const useBackendStore = defineStore('backends', () => {
  const profiles = ref<BackendProfile[]>([])
  const runtime = reactive<Record<string, BackendRuntime>>({})
  const loading = ref(false)
  const error = ref('')
  let task: Promise<void> | null = null
  const generations = new Map<string, number>()
  function state(id: string) {
    return (runtime[id] ??= {
      credential: { configured: false, persistence: 'missing' },
      models: [],
      checking: false,
      error: '',
    })
  }
  function invalidate(id: string) {
    generations.set(id, (generations.get(id) ?? 0) + 1)
    state(id).checking = false
    state(id).models = []
  }
  function isReady(id: string, revision: number) {
    const profile = profiles.value.find((p) => p.id === id)
    return (
      !!profile &&
      profile.revision === revision &&
      profile.config.enabled &&
      apiSupported(profile.config.kind) &&
      state(id).credential.configured
    )
  }
  async function load() {
    if (!isDesktop()) return
    if (task) return task
    task = (async () => {
      loading.value = true
      try {
        const result = await invoke<BackendProfile[]>('backend_profiles')
        profiles.value = result ?? []
        error.value = ''
        await Promise.all(
          profiles.value
            .filter((p) => p.config.kind !== 'codex')
            .map(async (p) => {
              const generation = generations.get(p.id) ?? 0
              try {
                const credential = await invoke<CredentialStatus>('backend_credential_status', {
                  profileId: p.id,
                })
                if ((generations.get(p.id) ?? 0) === generation) state(p.id).credential = credential
              } catch (e) {
                if ((generations.get(p.id) ?? 0) === generation) state(p.id).error = describe(e)
              }
            }),
        )
      } catch (e) {
        error.value = describe(e)
      } finally {
        loading.value = false
      }
    })()
    await task
    task = null
  }
  async function save(config: ProfileConfig, previous?: BackendProfile) {
    const profile = await invoke<BackendProfile>('backend_save_profile', {
      request: { id: previous?.id ?? null, expectedRevision: previous?.revision ?? null, config },
    })
    invalidate(profile.id)
    delete runtime[profile.id]
    if (task) await task
    await load()
    return profile
  }
  async function setCredential(profile: BackendProfile, key: string, persist: boolean) {
    const credential = await invoke<CredentialStatus>('backend_set_credential', {
      profileId: profile.id,
      revision: profile.revision,
      key,
      persist,
    })
    invalidate(profile.id)
    state(profile.id).credential = credential
    state(profile.id).error = ''
  }
  async function removeCredential(profile: BackendProfile) {
    await invoke('backend_remove_credential', { profileId: profile.id, revision: profile.revision })
    state(profile.id).credential = { configured: false, persistence: 'missing' }
    invalidate(profile.id)
  }
  async function check(profile: BackendProfile) {
    if (state(profile.id).checking) return
    const generation = (generations.get(profile.id) ?? 0) + 1
    generations.set(profile.id, generation)
    const valid = () =>
      generations.get(profile.id) === generation &&
      profiles.value.some((p) => p.id === profile.id && p.revision === profile.revision)
    state(profile.id).checking = true
    state(profile.id).error = ''
    try {
      const models = await invoke<string[]>('backend_models', { profileId: profile.id })
      if (valid()) state(profile.id).models = models
    } catch (e) {
      if (valid()) state(profile.id).error = describe(e)
    } finally {
      if (valid()) state(profile.id).checking = false
    }
  }
  return {
    profiles,
    runtime,
    loading,
    error,
    state,
    isReady,
    load,
    save,
    setCredential,
    removeCredential,
    check,
  }
})
