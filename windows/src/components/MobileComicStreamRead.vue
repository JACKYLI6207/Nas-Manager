<script setup lang="ts">
import {
  computed,
  nextTick,
  onActivated,
  onBeforeUnmount,
  onMounted,
  ref,
  toRefs,
  watch,
} from 'vue'
import { fetchRemoteComicPageImage, fetchRemoteComicPages } from '../api'
import {
  comicJobKey,
  comicStreamFailedIndices,
  comicStreamPageSrcMap,
  comicStreamReadSession,
  comicStreamSession,
  getFullComicStreamQueue,
  setCurrentComicStreamJob,
  type RemoteComicStreamJob,
} from '../comicStreamStore'
import {
  getComicStreamRecord,
  markComicStreamOpened,
  saveComicStreamPosition,
} from '../comicStreamProgress'
import { formatInvokeError } from '../invokeError'
import { useReaderAspectRatio } from '../composables/useReaderAspectRatio'
import { useReaderChromeAutoHide } from '../composables/useReaderChromeAutoHide'
import { useReaderFullscreen } from '../composables/useReaderFullscreen'
import {
  beginReaderRestore,
  clearReaderImageLoadProgress,
  endReaderRestore,
  endReaderSeek,
  isReaderProgressSeekDisabled,
  isReaderScrollLocked,
  notifyReaderSeekEnd,
  registerReaderScrollControl,
  unregisterReaderScrollControl,
  updateReaderImageLoadProgress,
  updateReaderScrollProgress,
} from '../readerProgressBridge'
import {
  approxScrollToCheckpoint,
  computeContentRatio,
  computeVisiblePageIndex,
  getPageElement,
  ratioToPageCheckpoint,
  recordToPageCheckpoint,
} from '../readerProgressMath'
import {
  beginRestoreMode,
  beginSeekSession,
  canObserverEnqueuePages,
  canSaveReadingCheckpoint,
  clearScrollIntent,
  commitPageCheckpoint,
  defaultEstimatedPageHeight,
  endRestoreMode,
  getScrollIntent,
  hasActiveScrollIntent,
  isPageImageDecoded,
  isRestoringMode,
  isStartCheckpoint,
  notifyPageImageReady,
  readCheckpointFromScroll,
  resetLocalReaderViewportState,
  RESTORE_NEIGHBOR_RADIUS,
  SCROLL_NEIGHBOR_RADIUS,
  setScrollIntent,
  waitForPageImageReady,
} from '../localReaderViewport'
import MobileReaderProgressBar from './MobileReaderProgressBar.vue'
import ReaderAspectRatioMenu from './ReaderAspectRatioMenu.vue'
import '../readerShared.css'

const PREFETCH_MARGIN = '600px 0px'

const { isFullscreen, toggleFullscreen, exitFullscreen } = useReaderFullscreen()
const {
  chromeVisible,
  onReaderScrollForChrome,
  onReaderTouchStart,
  onReaderTouchMove,
  onReaderTouchEnd,
  onReaderTouchCancel,
} = useReaderChromeAutoHide()
const { scrollClass: aspectScrollClass } = useReaderAspectRatio()

const s = comicStreamReadSession
const { readerPages, readerTitle, readingActive, status, openingBusy } = toRefs(s)
const estimatedPageHeight = ref(0)
const pageSrcMap = comicStreamPageSrcMap
const failedIndices = comicStreamFailedIndices
const scrollContainerRef = ref<HTMLElement | null>(null)
const activeJob = ref<RemoteComicStreamJob | null>(null)
let readerSessionId = 0
let observer: IntersectionObserver | undefined
let loadQueue: number[] = []
let activeLoads = 0
let scrollSaveTimer: ReturnType<typeof setTimeout> | undefined
let postSeekPrefetchTimer: ReturnType<typeof setTimeout> | undefined
let restoreSafetyTimer: ReturnType<typeof setTimeout> | undefined
let seekSafetyTimer: ReturnType<typeof setTimeout> | undefined
let restoreScrollSuppress = 0
let restoreSaveBlocked = false

const currentJob = computed(() => comicStreamSession.currentJob)
const fullQueue = computed(() => getFullComicStreamQueue())
const queueIndex = computed(() => {
  const job = activeJob.value
  if (!job) return -1
  const key = comicJobKey(job)
  return fullQueue.value.findIndex((j) => comicJobKey(j) === key)
})
const hasPrevBook = computed(() => queueIndex.value > 0)
const hasNextBook = computed(() => queueIndex.value >= 0 && queueIndex.value < fullQueue.value.length - 1)

function syncImageLoadProgress() {
  const total = readerPages.value.length
  if (!readingActive.value || total <= 0) {
    clearReaderImageLoadProgress()
    return
  }
  updateReaderImageLoadProgress({
    active: activeLoads > 0 || loadQueue.length > 0,
    loaded: pageSrcMap.value.size,
    total,
    failed: failedIndices.value.size,
  })
}

function updateScrollProgressBridge() {
  if (!readingActive.value || !activeJob.value) return
  if (hasActiveScrollIntent() || isReaderScrollLocked()) return
  const root = scrollContainerRef.value
  const total = readerPages.value.length
  if (!root || total <= 0) return
  syncImageLoadProgress()
  const pageIndex = computeVisiblePageIndex(root)
  const displayRatio = computeContentRatio(root, total, pageIndex)
  updateReaderScrollProgress({
    active: true,
    ratio: displayRatio,
    totalPages: total,
    currentPage: pageIndex + 1,
    pageLabel: `${pageIndex + 1}/${total}頁`,
  })
}

function persistScrollPosition() {
  const job = activeJob.value
  if (!job || !readingActive.value) return
  if (!canSaveReadingCheckpoint() || restoreSaveBlocked) return
  const root = scrollContainerRef.value
  const total = readerPages.value.length
  if (!root || total <= 0) return
  const snap = readCheckpointFromScroll(root, total, (i) => pageSrcMap.value.has(i))
  saveComicStreamPosition(
    job,
    snap.pageIndex,
    total,
    snap.scrollY,
    snap.offsetInPage,
    snap.offsetRatioInPage,
  )
}

function notePageHeight(index: number) {
  const root = scrollContainerRef.value
  if (!root) return
  const el = getPageElement(root, index)
  if (!el) return
  const h = el.offsetHeight || el.getBoundingClientRect().height
  if (h <= 0) return
  estimatedPageHeight.value = estimatedPageHeight.value
    ? Math.round(estimatedPageHeight.value * 0.75 + h * 0.25)
    : h
}

function clearPostSeekPrefetchTimer() {
  if (postSeekPrefetchTimer !== undefined) {
    clearTimeout(postSeekPrefetchTimer)
    postSeekPrefetchTimer = undefined
  }
}

function clearSeekSafetyTimer() {
  if (seekSafetyTimer !== undefined) {
    clearTimeout(seekSafetyTimer)
    seekSafetyTimer = undefined
  }
}

function finishSeekAfterCommit() {
  endReaderSeek()
  updateScrollProgressBridge()
  persistScrollPosition()
  notifyReaderSeekEnd()
}

function roughScrollForSeek(root: HTMLElement, cp: ReturnType<typeof ratioToPageCheckpoint>) {
  const est = defaultEstimatedPageHeight(estimatedPageHeight.value)
  restoreScrollSuppress += 1
  approxScrollToCheckpoint(root, cp, est)
  requestAnimationFrame(() => {
    restoreScrollSuppress = Math.max(0, restoreScrollSuppress - 1)
  })
}

function scheduleSeekSafetyTimeout() {
  clearSeekSafetyTimer()
  seekSafetyTimer = setTimeout(() => {
    seekSafetyTimer = undefined
    if (!hasActiveScrollIntent()) return
    clearScrollIntent()
    endReaderSeek()
    updateScrollProgressBridge()
  }, 45_000)
}

function schedulePostSeekPrefetch(pageIndex: number) {
  clearPostSeekPrefetchTimer()
  postSeekPrefetchTimer = setTimeout(() => {
    postSeekPrefetchTimer = undefined
    if (hasActiveScrollIntent()) return
    enqueueNeighborsInRadius(pageIndex, SCROLL_NEIGHBOR_RADIUS)
  }, 400)
}

function commitScrollIntentOnce(): boolean {
  const intent = getScrollIntent()
  const root = scrollContainerRef.value
  if (!intent || !root) return false
  const hasSrc = (i: number) => pageSrcMap.value.has(i)
  if (!isPageImageDecoded(root, intent.cp.pageIndex, hasSrc)) return false

  restoreScrollSuppress += 1
  const ok = commitPageCheckpoint(root, intent.cp, hasSrc)
  requestAnimationFrame(() => {
    restoreScrollSuppress = Math.max(0, restoreScrollSuppress - 1)
    if (ok) updateScrollProgressBridge()
  })
  return ok
}

/** 進度條鬆手：粗跳至估算位置，目標頁解碼後精準捲動一次 */
function seekByRatio(ratio: number) {
  if (isReaderProgressSeekDisabled() || isRestoringMode()) return
  const root = scrollContainerRef.value
  const total = readerPages.value.length
  if (!root || total <= 0) return

  const sessionId = beginSeekSession()
  const cp = ratioToPageCheckpoint(ratio, total)
  const hasSrc = (i: number) => pageSrcMap.value.has(i)

  clearPostSeekPrefetchTimer()
  clearSeekSafetyTimer()
  setScrollIntent('seek', cp, sessionId)
  roughScrollForSeek(root, cp)
  preloadPagesForRestore(cp.pageIndex)

  if (isPageImageDecoded(root, cp.pageIndex, hasSrc)) {
    commitScrollIntentOnce()
    clearScrollIntent()
    finishSeekAfterCommit()
    schedulePostSeekPrefetch(cp.pageIndex)
    return
  }

  scheduleSeekSafetyTimeout()
  enqueuePage(cp.pageIndex)
}

function stopRestoreSafetyTimer() {
  if (restoreSafetyTimer !== undefined) {
    clearTimeout(restoreSafetyTimer)
    restoreSafetyTimer = undefined
  }
}

function setScrollTopForRestore(root: HTMLElement, y: number) {
  restoreScrollSuppress += 1
  root.scrollTop = y
  requestAnimationFrame(() => {
    restoreScrollSuppress = Math.max(0, restoreScrollSuppress - 1)
  })
}

function preloadPagesForRestore(pageIndex: number) {
  const total = readerPages.value.length
  enqueuePage(pageIndex)
  for (let d = 1; d <= RESTORE_NEIGHBOR_RADIUS; d++) {
    if (pageIndex - d >= 0) enqueuePage(pageIndex - d)
    if (pageIndex + d < total) enqueuePage(pageIndex + d)
  }
}

function finishPositionRestore() {
  if (!isRestoringMode()) return
  stopRestoreSafetyTimer()
  endReaderRestore()
  endRestoreMode()
  restoreSaveBlocked = true
  persistScrollPosition()
  window.setTimeout(() => {
    restoreSaveBlocked = false
  }, 2500)
  updateScrollProgressBridge()
}

function abortRestoreOverlay() {
  if (!isRestoringMode()) return
  stopRestoreSafetyTimer()
  endRestoreMode()
  clearScrollIntent()
  restoreSaveBlocked = true
  window.setTimeout(() => {
    restoreSaveBlocked = false
  }, 2500)
  endReaderRestore()
  updateScrollProgressBridge()
}

async function restoreReadingPosition() {
  stopRestoreSafetyTimer()
  const job = activeJob.value
  const rec = job ? getComicStreamRecord(job) : null
  const total = readerPages.value.length
  if (!rec?.opened || total === 0) {
    enqueueNeighborsInRadius(0, SCROLL_NEIGHBOR_RADIUS)
    return
  }

  const pageIndex = Math.min(Math.max(0, rec.readPage), total - 1)

  if (isStartCheckpoint(rec)) {
    enqueueNeighborsInRadius(0, SCROLL_NEIGHBOR_RADIUS)
    await nextTick()
    const root = scrollContainerRef.value
    if (root) setScrollTopForRestore(root, 0)
    updateScrollProgressBridge()
    return
  }

  const cp = recordToPageCheckpoint(rec, defaultEstimatedPageHeight(estimatedPageHeight.value))
  const sessionAtStart = readerSessionId
  const restoreSessionId = beginSeekSession()

  beginRestoreMode(pageIndex)
  beginReaderRestore()
  setScrollIntent('restore', cp, restoreSessionId)

  preloadPagesForRestore(pageIndex)
  await nextTick()

  const root = scrollContainerRef.value
  const hasSrc = (i: number) => pageSrcMap.value.has(i)
  if (root && isPageImageDecoded(root, pageIndex, hasSrc)) {
    commitScrollIntentOnce()
    clearScrollIntent()
    finishPositionRestore()
    return
  }

  restoreSafetyTimer = setTimeout(() => {
    if (isRestoringMode()) abortRestoreOverlay()
  }, 45_000)

  const ok = await waitForPageImageReady(pageIndex, () => sessionAtStart === readerSessionId)
  if (!isRestoringMode() || sessionAtStart !== readerSessionId) return
  if (!ok) abortRestoreOverlay()
}

function onReaderScroll() {
  onReaderScrollForChrome()
  if (!readingActive.value) return
  if (isRestoringMode() || hasActiveScrollIntent() || restoreScrollSuppress > 0) return
  if (isReaderScrollLocked()) return
  updateScrollProgressBridge()
  if (scrollSaveTimer !== undefined) clearTimeout(scrollSaveTimer)
  scrollSaveTimer = setTimeout(() => persistScrollPosition(), 500)
}

function maxConcurrentLoads() {
  return 2
}

function setupObserver() {
  observer?.disconnect()
  const root = scrollContainerRef.value
  if (!root) return
  observer = new IntersectionObserver(
    (entries) => {
      if (!canObserverEnqueuePages()) return
      for (const entry of entries) {
        if (!entry.isIntersecting) continue
        const index = Number((entry.target as HTMLElement).dataset.index)
        if (!Number.isNaN(index)) enqueueNeighbors(index)
      }
    },
    { root, rootMargin: PREFETCH_MARGIN, threshold: 0 },
  )
}

function setPageRef(index: number, el: Element | { $el?: unknown } | null) {
  const node =
    el instanceof HTMLElement ? el : (el as { $el?: unknown } | null)?.$el
  if (!(node instanceof HTMLElement)) return
  observer?.observe(node)
}

function enqueueNeighborsInRadius(center: number, radius: number) {
  const total = readerPages.value.length
  for (let delta = -radius; delta <= radius; delta++) {
    const index = center + delta
    if (index >= 0 && index < total) enqueuePage(index)
  }
}

function enqueueNeighbors(center: number) {
  enqueueNeighborsInRadius(center, SCROLL_NEIGHBOR_RADIUS)
}

function pumpLoadQueue() {
  while (activeLoads < maxConcurrentLoads() && loadQueue.length > 0) {
    const index = loadQueue.shift()
    if (index === undefined) return
    void loadPage(index)
  }
}

function enqueuePage(index: number) {
  if (!readingActive.value || !activeJob.value) return
  if (pageSrcMap.value.has(index) || loadQueue.includes(index)) return
  if (failedIndices.value.has(index)) {
    const next = new Set(failedIndices.value)
    next.delete(index)
    failedIndices.value = next
  }
  loadQueue.push(index)
  syncImageLoadProgress()
  pumpLoadQueue()
}

async function loadPage(index: number) {
  const session = readerSessionId
  const job = activeJob.value
  const page = readerPages.value[index]
  if (!readingActive.value || !job || !page || pageSrcMap.value.has(index)) return

  activeLoads += 1
  try {
    const bytes = await fetchRemoteComicPageImage(job.host, job.port, job.relPath, page.entry)
    if (session !== readerSessionId || !readingActive.value) return
    const blob = new Blob([new Uint8Array(bytes)])
    const src = URL.createObjectURL(blob)
    const nextMap = new Map(pageSrcMap.value)
    nextMap.set(index, src)
    pageSrcMap.value = nextMap
    syncImageLoadProgress()
    requestAnimationFrame(() => notePageHeight(index))
  } catch {
    if (session === readerSessionId) {
      failedIndices.value = new Set(failedIndices.value).add(index)
      syncImageLoadProgress()
    }
  } finally {
    activeLoads -= 1
    syncImageLoadProgress()
    pumpLoadQueue()
  }
}

function pendingPageStyle(index: number): Record<string, string> | undefined {
  if (pageSrcMap.value.has(index) || failedIndices.value.has(index)) return undefined
  const h = estimatedPageHeight.value
  const minH = h > 0 ? h : Math.max(320, Math.floor(window.innerHeight * 0.72))
  return { minHeight: `${minH}px` }
}

function onPageImageLoad(index: number) {
  if (!readingActive.value || !activeJob.value) return

  notifyPageImageReady(index)
  notePageHeight(index)

  const intent = getScrollIntent()
  if (!intent || intent.cp.pageIndex !== index) {
    if (!isRestoringMode() && !hasActiveScrollIntent()) updateScrollProgressBridge()
    return
  }

  requestAnimationFrame(() => {
    if (!commitScrollIntentOnce()) return

    const center = intent.cp.pageIndex
    clearScrollIntent()

    if (intent.kind === 'restore' && isRestoringMode()) {
      finishPositionRestore()
      return
    }

    if (intent.kind === 'seek') {
      clearSeekSafetyTimer()
      finishSeekAfterCommit()
      schedulePostSeekPrefetch(center)
    }
  })
}

async function openJob(job: RemoteComicStreamJob) {
  openingBusy.value = true
  status.value = '正在載入頁面清單…'
  try {
    const result = await fetchRemoteComicPages(job.host, job.port, job.relPath)
    readerSessionId += 1
    resetLocalReaderViewportState()
    estimatedPageHeight.value = 0
    clearPostSeekPrefetchTimer()
    clearSeekSafetyTimer()
    stopRestoreSafetyTimer()
    pageSrcMap.value.forEach((url) => URL.revokeObjectURL(url))
    pageSrcMap.value = new Map()
    failedIndices.value = new Set()
    loadQueue = []
    activeLoads = 0

    activeJob.value = { ...job }
    setCurrentComicStreamJob(job)
    readerTitle.value = result.title || job.title
    readerPages.value = result.pages.map((p) => ({
      index: p.index,
      caption: p.caption,
      entry: p.entry,
    }))
    readingActive.value = true
    markComicStreamOpened(job, result.pages.length)
    status.value = ''

    await nextTick()
    setupObserver()
    await restoreReadingPosition()
    updateScrollProgressBridge()
  } catch (e) {
    status.value = formatInvokeError(e)
    readingActive.value = false
  } finally {
    openingBusy.value = false
    comicStreamSession.pendingRead = false
  }
}

function closeReading() {
  persistScrollPosition()
  resetLocalReaderViewportState()
  clearPostSeekPrefetchTimer()
  clearSeekSafetyTimer()
  stopRestoreSafetyTimer()
  readingActive.value = false
  readerPages.value = []
  readerTitle.value = ''
  activeJob.value = null
  pageSrcMap.value.forEach((url) => URL.revokeObjectURL(url))
  pageSrcMap.value = new Map()
  failedIndices.value = new Set()
  loadQueue = []
  observer?.disconnect()
  unregisterReaderScrollControl()
  clearReaderImageLoadProgress()
  exitFullscreen()
}

function goToAdjacentSource(delta: number) {
  const list = getFullComicStreamQueue()
  const job = activeJob.value ?? comicStreamSession.currentJob
  if (!job) return
  const idx = list.findIndex((j) => comicJobKey(j) === comicJobKey(job))
  if (idx < 0) return
  const next = list[idx + delta]
  if (!next) return
  closeReading()
  setCurrentComicStreamJob(next)
  comicStreamSession.pendingRead = true
}

async function tryOpenPending() {
  if (!comicStreamSession.pendingRead) return
  const job = comicStreamSession.currentJob
  if (!job || readingActive.value || openingBusy.value) return
  await openJob(job)
}

watch(
  () => comicStreamSession.pendingRead,
  (v) => {
    if (v) void tryOpenPending()
  },
)

watch(
  () => comicStreamSession.currentJob,
  (job) => {
    if (comicStreamSession.pendingRead && job && !readingActive.value) {
      void tryOpenPending()
    }
  },
)

watch(
  readingActive,
  (active) => {
    if (active) {
      registerReaderScrollControl({
        seek: seekByRatio,
        savePosition: () => {
          persistScrollPosition()
          updateScrollProgressBridge()
        },
      })
      updateScrollProgressBridge()
    } else {
      unregisterReaderScrollControl()
    }
  },
  { immediate: true },
)

onMounted(() => {
  void tryOpenPending()
})

onActivated(() => {
  void tryOpenPending()
})

onBeforeUnmount(() => {
  if (scrollSaveTimer !== undefined) clearTimeout(scrollSaveTimer)
  clearPostSeekPrefetchTimer()
  clearSeekSafetyTimer()
  stopRestoreSafetyTimer()
  persistScrollPosition()
  observer?.disconnect()
  unregisterReaderScrollControl()
})
</script>

<template>
  <div v-if="readingActive" class="stream-read-root">
    <div :class="['reader-shell', { 'reader-shell--fullscreen': isFullscreen }]">
      <div
        v-if="isFullscreen"
        class="reader-top-chrome"
        :class="{ 'reader-top-chrome--hidden': !chromeVisible }"
      >
        <div class="reader-top-title" :title="readerTitle">{{ readerTitle }}</div>
      </div>
      <div
        ref="scrollContainerRef"
        class="reader-scroll"
        :class="aspectScrollClass"
        @scroll="onReaderScroll"
        @touchstart.passive="onReaderTouchStart"
        @touchmove.passive="onReaderTouchMove"
        @touchend="onReaderTouchEnd"
        @touchcancel="onReaderTouchCancel"
      >
        <div
          v-for="(page, index) in readerPages"
          :key="`${readerSessionId}-${index}`"
          :ref="(el) => setPageRef(index, el)"
          class="reader-page"
          :class="{ 'reader-page--pending': !pageSrcMap.get(index) && !failedIndices.has(index) }"
          :style="pendingPageStyle(index)"
          :data-index="index"
        >
          <img
            v-if="pageSrcMap.get(index)"
            :src="pageSrcMap.get(index)"
            :alt="page.caption"
            class="reader-img"
            @load="onPageImageLoad(index)"
          />
          <p v-else-if="failedIndices.has(index)" class="reader-ph">載入失敗，請向下捲動重試</p>
        </div>
      </div>
      <div
        class="reader-bottom-chrome"
        :class="{
          'reader-bottom-chrome--fullscreen': isFullscreen,
          'reader-bottom-chrome--hidden': isFullscreen && !chromeVisible,
        }"
      >
        <div class="reader-header reader-header--bottom">
          <ReaderAspectRatioMenu />
          <div class="reader-header-actions">
            <template v-if="fullQueue.length > 1">
              <button
                type="button"
                class="reader-btn"
                :disabled="!hasPrevBook || openingBusy"
                @click="goToAdjacentSource(-1)"
              >
                上一本
              </button>
              <button
                type="button"
                class="reader-btn"
                :disabled="!hasNextBook || openingBusy"
                @click="goToAdjacentSource(1)"
              >
                下一本
              </button>
            </template>
            <button type="button" class="reader-btn" :disabled="openingBusy" @click="toggleFullscreen">
              {{ isFullscreen ? '視窗模式' : '全視窗' }}
            </button>
            <button type="button" class="reader-btn" :disabled="openingBusy" @click="closeReading">關閉</button>
          </div>
        </div>
      <MobileReaderProgressBar v-if="!isFullscreen" />
    </div>
    </div>
  </div>
</template>

<style scoped>
.stream-read-root {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
}
</style>
