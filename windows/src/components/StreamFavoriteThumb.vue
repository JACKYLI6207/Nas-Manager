<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  invalidateStreamFavoriteCover,
  resolveStreamFavoriteCoverUrl,
  streamFavoriteCoverEpoch,
  type StreamFavoriteCoverKind,
} from '../streamFavoriteCover'
import { entryFallbackIcon } from '../remoteBrowseMedia'
import type { RemoteStreamJob } from '../videoStreamStore'

const props = defineProps<{
  job: RemoteStreamJob
  kind: StreamFavoriteCoverKind
}>()

const thumbSrc = ref<string | null>(null)
const loadState = ref<'idle' | 'loading' | 'ready' | 'fail'>('idle')
const rootRef = ref<HTMLElement | null>(null)
let observer: IntersectionObserver | undefined
let requestSeq = 0
let retryCount = 0

function fallbackEmoji(): string {
  return entryFallbackIcon(props.job.relPath, false)
}

async function loadThumb() {
  const seq = ++requestSeq
  loadState.value = 'loading'
  try {
    const url = await resolveStreamFavoriteCoverUrl(props.job, props.kind)
    if (seq !== requestSeq) return
    if (url) {
      thumbSrc.value = url
      loadState.value = 'ready'
      retryCount = 0
    } else if (retryCount < 1) {
      retryCount += 1
      invalidateStreamFavoriteCover(props.job, props.kind)
      window.setTimeout(() => {
        if (seq === requestSeq) void loadThumb()
      }, 400)
    } else {
      thumbSrc.value = null
      loadState.value = 'fail'
    }
  } catch {
    if (seq !== requestSeq) return
    thumbSrc.value = null
    loadState.value = 'fail'
  }
}

function onImgError() {
  invalidateStreamFavoriteCover(props.job, props.kind)
  thumbSrc.value = null
  if (retryCount < 1) {
    retryCount += 1
    void loadThumb()
  } else {
    loadState.value = 'fail'
  }
}

function setupObserver() {
  observer?.disconnect()
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
  () =>
    [
      props.job.host,
      props.job.port,
      props.job.relPath,
      props.kind,
      streamFavoriteCoverEpoch.value,
    ] as const,
  () => {
    setupObserver()
  },
)

onMounted(() => {
  setupObserver()
})

onBeforeUnmount(() => {
  observer?.disconnect()
})
</script>

<template>
  <div ref="rootRef" class="fav-thumb">
    <img
      v-if="thumbSrc"
      :src="thumbSrc"
      alt=""
      class="fav-thumb-img"
      loading="lazy"
      decoding="async"
      @error="onImgError"
    />
    <span v-else-if="loadState === 'loading'" class="fav-thumb-loading" aria-hidden="true" />
    <span v-else class="fav-thumb-fallback">{{ fallbackEmoji() }}</span>
  </div>
</template>

<style scoped>
.fav-thumb {
  width: 100%;
  aspect-ratio: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.06);
}

.fav-thumb-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  object-position: center;
  display: block;
}

.fav-thumb-fallback {
  font-size: clamp(22px, 40%, 48px);
  line-height: 1;
  user-select: none;
}

.fav-thumb-loading {
  width: 28%;
  aspect-ratio: 1;
  border-radius: 50%;
  border: 2px solid rgba(255, 255, 255, 0.15);
  border-top-color: rgba(110, 181, 255, 0.75);
  animation: fav-thumb-spin 0.8s linear infinite;
}

@keyframes fav-thumb-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
