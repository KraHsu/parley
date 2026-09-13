<script setup lang="ts">
export type IconName =
  | 'chat'
  | 'book'
  | 'sparkles'
  | 'arrow-up'
  | 'arrow-right'
  | 'chevron'
  | 'settings'
  | 'globe'
  | 'sun'
  | 'compass'
  | 'coffee'
  | 'close'
  | 'search'
  | 'plug'
  | 'check'
  | 'translate'
  | 'quote'
  | 'leaf'

defineProps<{ name: IconName; size?: number }>()

const paths: Record<IconName, string> = {
  chat: 'M21 11.5a8.4 8.4 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.4 8.4 0 0 1 3.8-.9h.5a8.5 8.5 0 0 1 8 8v.5Z',
  book: 'M4 3h6a3 3 0 0 1 3 3v15a4 4 0 0 0-4-2H3V4a1 1 0 0 1 1-1Zm9 3a3 3 0 0 1 3-3h5v16h-4a4 4 0 0 0-4 2',
  sparkles: 'm12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5L12 3Zm7-1v4m-2-2h4',
  'arrow-up': 'M12 19V5m-6 6 6-6 6 6',
  'arrow-right': 'M5 12h14m-6-6 6 6-6 6',
  chevron: 'm8 10 4 4 4-4',
  settings:
    'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm-2-5h4l.7 2.3 2 .9 2.3-.5 2 3.5-1.6 1.7v2.2l1.6 1.7-2 3.5-2.3-.5-2 .9L14 21h-4l-.7-2.3-2-.9-2.3.5-2-3.5 1.6-1.7v-2.2L3 9.2l2-3.5 2.3.5 2-.9L10 3Z',
  globe: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM3 12h18M12 3c4 5 4 13 0 18-4-5-4-13 0-18Z',
  sun: 'M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0ZM12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5',
  compass: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0Zm-5-4-2 6-6 2 2-6 6-2Z',
  coffee: 'M4 8h12v7a4 4 0 0 1-4 4H8a4 4 0 0 1-4-4V8Zm12 1h2a3 3 0 1 1 0 6h-2M7 2v2m4-2v2M2 22h18',
  close: 'm6 6 12 12M6 18 18 6',
  search: 'M20 20l-5-5m2-5a7 7 0 1 1-14 0 7 7 0 0 1 14 0Z',
  plug: 'm8 3 2 4m6-4-2 4M6 7h12v3a6 6 0 0 1-12 0V7Zm6 9v5',
  check: 'm5 12 4 4L19 6',
  translate: 'M3 5h12M9 3v2M5 5c0 5 4 8 8 10M13 5c0 5-4 8-8 10m8 6 4-10 4 10m-6.5-4h5',
  quote: 'M10 6H4v6h5c0 3-2 5-4 6m15-12h-6v6h5c0 3-2 5-4 6',
  leaf: 'M20 3C8 3 3 8 5 15c2 7 15 5 15-12ZM4 21l11-11',
}
</script>

<template>
  <svg
    :width="size ?? 20"
    :height="size ?? 20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.65"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path :d="paths[name]" />
  </svg>
</template>
