<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { fetchRemoteComicPageImage, fetchRemoteComicPages, MIN_COMIC_REMOTE_API } from '../api'
import {
  buildRemoteStreamUrl,
  entryFallbackIcon,
  getCachedThumb,
  isComicZipFileName,
  isImageFile,
  setCachedThumb,
} from '../remoteBrowseMedia'

const props = defineProps<{
  host: string
  port: number
  relPath: string
  name: string
  isDir: boolean
  remoteApi?: number | null
}>()

const thumbSrc = ref<string | null>(null)
const rootRef = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | undefined
let ownedBlobUrl: string | null = null

function cacheKey(): string {
  return `${props.host}:${props.port}:${props.relPath}`
}

function showFallback(): boolean {
  return !thumbSrc.value
}

function fallbackEmoji(): string {
  return entryFallbackIcon(props.name, props.isDir)
}

async function loadThumb() {
  if (props.isDir) return
  const key = cacheKey()
  const cached = getCachedThumb(key)
  if (cached) {
    thumbSrc.value = cached
    return
  }

  if (isImageFile(props.name)) {
    const url = buildRemoteStreamUrl(props.host, props.port, props.relPath)
    setCachedThumb(key, url)
    thumbSrc.value = url
    return
  }

  if (isComicZipFileName(props.name) && (props.remoteApi ?? 0) >= MIN_COMIC_REMOTE_API) {
    const pages = await fetchRemoteComicPages(props.host, props.port, props.relPath)
    const first = pages.pages[0]
    if (!first) return
    const bytes = await fetchRemoteComicPageImage(
      props.host,
      props.port,
      props.relPath,
      first.entry,
    )
    const blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' })
    ownedBlobUrl = URL.createObjectURL(blob)
    setCachedThumb(key, ownedBlobUrl)
    thumbSrc.value = ownedBlobUrl
  }
}

function setupObserver() {
  observer?.disconnect()
  if (props.isDir) return
  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        void loadThumb().catch(() => {
          /* fallback emoji */
        })
        observer?.disconnect()
        observer = undefined
      }
    },
    { rootMargin: '160px' },
  )
  if (rootRef.value) {
    observer.observe(rootRef.value)
  }
}

watch(
  () => [props.host, props.port, props.relPath, props.name] as const,
  () => {
    if (ownedBlobUrl) {
      URL.revokeObjectURL(ownedBlobUrl)
      ownedBlobUrl = null
    }
    thumbSrc.value = null
    setupObserver()
  },
)

onMounted(() => {
  setupObserver()
})

onBeforeUnmount(() => {
  observer?.disconnect()
  if (ownedBlobUrl) {
    URL.revokeObjectURL(ownedBlobUrl)
    ownedBlobUrl = null
  }
})
</script>

<template>
  <div ref="rootRef" class="remote-browse-thumb" :class="{ 'remote-browse-thumb--dir': isDir }">
    <img
      v-if="thumbSrc"
      :src="thumbSrc"
      alt=""
      class="remote-browse-thumb-img"
      loading="lazy"
      decoding="async"
    />
    <span v-else-if="showFallback()" class="remote-browse-thumb-fallback">{{ fallbackEmoji() }}</span>
  </div>
</template>

<style scoped>
.remote-browse-thumb {
  width: 100%;
  aspect-ratio: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.06);
}

.remote-browse-thumb--dir {
  background: rgba(255, 204, 0, 0.12);
}

.remote-browse-thumb-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  object-position: center;
  display: block;
}

.remote-browse-thumb-fallback {
  font-size: clamp(22px, 40%, 48px);
  line-height: 1;
  user-select: none;
}
</style>
