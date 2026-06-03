import { computed, ref } from 'vue'

export type ReaderAspectRatio = 'full' | '4:3' | '16:9'

const STORAGE_KEY = 'gm-reader-aspect-ratio'

export const ASPECT_RATIO_OPTIONS: { id: ReaderAspectRatio; label: string }[] = [
  { id: '4:3', label: '4:3' },
  { id: '16:9', label: '16:9' },
  { id: 'full', label: '全螢幕' },
]

function readStored(): ReaderAspectRatio {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw === '4:3' || raw === '16:9' || raw === 'full') return raw
  } catch {
    /* ignore */
  }
  return 'full'
}

export const readerAspectRatio = ref<ReaderAspectRatio>(readStored())

export function setReaderAspectRatio(mode: ReaderAspectRatio) {
  readerAspectRatio.value = mode
  try {
    localStorage.setItem(STORAGE_KEY, mode)
  } catch {
    /* ignore */
  }
}

export function useReaderAspectRatio() {
  const scrollClass = computed(() => {
    switch (readerAspectRatio.value) {
      case '4:3':
        return 'reader-scroll--ratio-4-3'
      case '16:9':
        return 'reader-scroll--ratio-16-9'
      default:
        return ''
    }
  })

  return {
    aspectRatio: readerAspectRatio,
    scrollClass,
    setAspectRatio: setReaderAspectRatio,
    options: ASPECT_RATIO_OPTIONS,
  }
}
