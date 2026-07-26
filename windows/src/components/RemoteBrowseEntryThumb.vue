<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { fetchRemoteComicPageImage, fetchRemoteComicPages, MIN_COMIC_REMOTE_API } from '../api'
import {
  resolveSiblingCoverUrl,
  VIDEO_LIST_MARKER,
} from '../remoteBrowseCover'
import {
  buildRemoteStreamUrl,
  entryFallbackIcon,
  getCachedThumb,
  isComicZipFileName,
  isImageFile,
  isVideoFileName,
  removeCachedThumb,
  scheduleThumbWork,
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
const loadState = ref<'idle' | 'loading' | 'ready' | 'fail'>('idle')
const rootRef = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | undefined
let ownedBlobUrl: string | null = null
let requestSeq = 0
let retryCount = 0

function cacheKey(): string {
  return `${props.host}:${props.port}:${props.relPath}`
}

function fallbackEmoji(): string {
  return entryFallbackIcon(props.name, props.isDir)
}

async function resolveThumbUrl(): Promise<string | null> {
  if (props.isDir) return null

  const key = cacheKey()
  const cached = getCachedThumb(key)
  if (cached) return cached

  if (isImageFile(props.name)) {
    return buildRemoteStreamUrl(props.host, props.port, props.relPath)
  }

  if (isVideoFileName(props.name)) {
    // 穩定 URL（不加 Date.now），讓瀏覽器可重用連線／快取，避免進目錄時打爆 PC
    return resolveSiblingCoverUrl(
      props.host,
      props.port,
      props.relPath,
      VIDEO_LIST_MARKER,
    )
  }

  if (isComicZipFileName(props.name) && (props.remoteApi ?? 0) >= MIN_COMIC_REMOTE_API) {
    const pages = await fetchRemoteComicPages(props.host, props.port, props.relPath)
    const first = pages.pages[0]
    if (!first) return null
    const bytes = await fetchRemoteComicPageImage(
      props.host,
      props.port,
      props.relPath,
      first.entry,
    )
    const blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' })
    if (ownedBlobUrl) {
      URL.revokeObjectURL(ownedBlobUrl)
    }
    ownedBlobUrl = URL.createObjectURL(blob)
    return ownedBlobUrl
  }

  return null
}

async function loadThumb() {
  if (props.isDir) return
  const seq = ++requestSeq
  loadState.value = 'loading'
  try {
    const url = await scheduleThumbWork(() => resolveThumbUrl())
    if (seq !== requestSeq) return
    if (url) {
      thumbSrc.value = url
      loadState.value = 'ready'
    } else if (retryCount < 2) {
      retryCount += 1
      window.setTimeout(() => {
        if (seq === requestSeq) void loadThumb()
      }, 280 * retryCount)
    } else {
      thumbSrc.value = null
      loadState.value = 'fail'
    }
  } catch {
    if (seq !== requestSeq) return
    if (retryCount < 2) {
      retryCount += 1
      window.setTimeout(() => {
        if (seq === requestSeq) void loadThumb()
      }, 280 * retryCount)
    } else {
      thumbSrc.value = null
      loadState.value = 'fail'
    }
  }
}

function onImgLoad() {
  const url = thumbSrc.value
  if (!url) return
  setCachedThumb(cacheKey(), url)
  retryCount = 0
  loadState.value = 'ready'
}

function onImgError() {
  const key = cacheKey()
  removeCachedThumb(key)
  const failed = thumbSrc.value
  thumbSrc.value = null
  if (failed?.startsWith('blob:') && failed === ownedBlobUrl) {
    try {
      URL.revokeObjectURL(failed)
    } catch {
      /* ignore */
    }
    ownedBlobUrl = null
  }
  if (retryCount < 2) {
    retryCount += 1
    window.setTimeout(() => {
      void loadThumb()
    }, 320 * retryCount)
  } else {
    loadState.value = 'fail'
  }
}

function setupObserver() {
  observer?.disconnect()
  if (props.isDir) {
    loadState.value = 'idle'
    thumbSrc.value = null
    return
  }
  retryCount = 0
  loadState.value = 'idle'
  thumbSrc.value = null
  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        void loadThumb()
        observer?.disconnect()
        observer = undefined
      }
    },
    { rootMargin: '200px' },
  )
  if (rootRef.value) {
    observer.observe(rootRef.value)
  }
}

watch(
  () => [props.host, props.port, props.relPath, props.name] as const,
  () => {
    requestSeq += 1
    if (ownedBlobUrl) {
      URL.revokeObjectURL(ownedBlobUrl)
      ownedBlobUrl = null
    }
    setupObserver()
  },
)

onMounted(() => {
  setupObserver()
})

onBeforeUnmount(() => {
  observer?.disconnect()
  requestSeq += 1
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
      decoding="async"
      @load="onImgLoad"
      @error="onImgError"
    />
    <span v-else-if="loadState === 'loading'" class="remote-browse-thumb-loading" aria-hidden="true" />
    <span v-else-if="loadState === 'fail' || isDir" class="remote-browse-thumb-fallback">{{
      fallbackEmoji()
    }}</span>
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

.remote-browse-thumb-loading {
  width: 28%;
  aspect-ratio: 1;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.15);
  border-top-color: rgba(110, 181, 255, 0.75);
  animation: remote-browse-thumb-spin 0.8s linear infinite;
}

@keyframes remote-browse-thumb-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
