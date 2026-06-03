<script setup lang="ts">
import {
  computed,
  nextTick,
  onActivated,
  onBeforeUnmount,
  onDeactivated,
  onMounted,
  ref,
  toRefs,
  watch,
} from 'vue'
import {
  closeLocalReaderZipSession,
  getLocalReaderImage,
  listLocalReaderSources,
  loadLocalReaderPages,
  pickLocalReaderFolder,
  pickLocalReaderZip,
  prepareLocalReaderZip,
  type LocalReaderPages,
} from '../api'
import {
  cancelFolderListMode,
  clearSourceReadRecord,
  formatSourceProgressLabel,
  getSourceRecord,
  hasSourceReadRecord,
  loadFolderSourceProgress,
  localReadFailedIndices,
  localReadPageSrcMap,
  localReadSession,
  markSourceOpened,
  saveSourceReadPosition,
  touchFolderPersist,
} from '../localReadStore'
import { decodeFolderDisplayLabel, isGmSnapCacheTitle, pickLocalReaderTitle } from '../readerDisplayName'
import { useReaderAspectRatio } from '../composables/useReaderAspectRatio'
import { useReaderChromeAutoHide } from '../composables/useReaderChromeAutoHide'
import { useReaderFullscreen } from '../composables/useReaderFullscreen'
import {
  beginReaderRestore,
  endReaderRestore,
  isReaderProgressSeekDisabled,
  isReaderScrollLocked,
  registerReaderScrollControl,
  unregisterReaderScrollControl,
  clearReaderImageLoadProgress,
  updateReaderImageLoadProgress,
  updateReaderScrollProgress,
} from '../readerProgressBridge'
import {
  computeContentRatio,
  computeVisiblePageIndex,
  getPageElement,
  getPageHeight,
  ratioToPageCheckpoint,
  recordToPageCheckpoint,
} from '../readerProgressMath'
import {
  beginRestoreMode,
  beginSeekSession,
  canEnqueuePage,
  canObserverEnqueuePages,
  canSaveReadingCheckpoint,
  clearScrollIntent,
  commitPageCheckpoint,
  defaultEstimatedPageHeight,
  endRestoreMode,
  getRestoreBookmarkPage,
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
import { comicReadSubTab } from '../comicReadUi'
import {
  comicStreamReadSession,
  comicStreamSession,
} from '../comicStreamStore'
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

const s = localReadSession
const {
  readerPages,
  readerTitle,
  readingActive,
  pickingSource,
  sourceListMode,
  folderLabel,
  folderSources,
  currentSourceIndex,
  currentSourcePath,
  status,
} = toRefs(s)

const openingBusy = ref(false)
/** 還原剛結束時短暫禁止寫入，避免錯誤位置覆蓋書籤 */
let restoreSaveBlocked = false
const estimatedPageHeight = ref(0)
/** 等待系統 SAF 選擇器時不可顯示全螢幕遮罩，否則會擋住選檔視窗 */
const awaitingSystemPicker = ref(false)
const openProgressLabel = ref('')
const openProgressPercent = ref(0)
const openProgressIndeterminate = ref(false)
const pageSrcMap = localReadPageSrcMap
const failedIndices = localReadFailedIndices
const scrollContainerRef = ref<HTMLElement | null>(null)
let readerSessionId = 0
let observer: IntersectionObserver | undefined
let loadQueue: number[] = []
let activeLoads = 0
let progressTimer: ReturnType<typeof setInterval> | undefined
let openQueue: Promise<void> = Promise.resolve()
let openGeneration = 0
/** 資料夾內多篇章模式（取消前保持，避免 sourceListMode 被誤清） */
const folderMultiZipMode = ref(false)

const streamCurrentJob = computed(() => comicStreamSession.currentJob)
const streamOpeningBusy = computed(() => comicStreamReadSession.openingBusy)
const streamStatus = computed(() => comicStreamReadSession.status)

function startStreamReadFromHome() {
  const job = comicStreamSession.currentJob
  if (!job || streamOpeningBusy.value) return
  comicReadSubTab.value = 'home'
  comicStreamSession.pendingRead = true
}

let scrollSaveTimer: ReturnType<typeof setTimeout> | undefined
let restoreSafetyTimer: ReturnType<typeof setTimeout> | undefined
let restoreScrollSuppress = 0
let folderPickGeneration = 0
let pickerVisibilityTimer: ReturnType<typeof setTimeout> | undefined
let pickerSafetyTimer: ReturnType<typeof setTimeout> | undefined
let pickerWentHidden = false
let activePickCancel: ((msg?: string) => void) | null = null

const READER_ERROR_HINTS: Record<string, string> = {
  '選擇資料夾逾時，請重試':
    '選擇資料夾逾時：系統尚未回傳結果。請再按「開啟資料夾」重試；若仍失敗請重開 App 後再選一次。',
  '選擇 ZIP 逾時，請重試':
    '選擇 ZIP 逾時：系統尚未回傳結果。請再按「開啟 ZIP 檔」重試。',
  '選擇未完成，請再試一次':
    '選擇未完成：可能未收到系統回傳。請再按「開啟資料夾」重試。',
  '選擇逾時，請再試一次':
    '選擇逾時：請再按「開啟資料夾」重試；若持續失敗請重開 App。',
  '掃描資料夾逾時，請重試':
    '掃描資料夾逾時：資料夾內容較多，請稍後再試或改選較小的資料夾。',
  '載入逾時，請重試': '載入逾時：檔案較大或較多，請稍後再試。',
  '已取消選擇': '已取消選擇。',
}

function formatReaderError(e: unknown): string {
  const raw =
    e instanceof Error ? e.message : typeof e === 'string' ? e : String(e ?? '未知錯誤')
  const msg = raw.replace(/^Error:\s*/i, '').trim()
  return READER_ERROR_HINTS[msg] ?? msg
}

function clearPickerTimers() {
  if (pickerVisibilityTimer !== undefined) {
    clearTimeout(pickerVisibilityTimer)
    pickerVisibilityTimer = undefined
  }
  if (pickerSafetyTimer !== undefined) {
    clearTimeout(pickerSafetyTimer)
    pickerSafetyTimer = undefined
  }
}

function releaseOpenBusyState() {
  stopProgressAnimation()
  openingBusy.value = false
  awaitingSystemPicker.value = false
  openProgressIndeterminate.value = false
  openProgressPercent.value = 0
  openProgressLabel.value = ''
}

function onDocumentVisibilityChange() {
  if (!awaitingSystemPicker.value) return
  if (document.visibilityState === 'hidden') {
    pickerWentHidden = true
    if (pickerVisibilityTimer !== undefined) {
      clearTimeout(pickerVisibilityTimer)
      pickerVisibilityTimer = undefined
    }
    return
  }
  // 僅在曾離開 App（開啟系統選擇器）後回到前景才啟動寬限；避免部分機型未觸發 hidden 而誤判逾時
  if (!pickerWentHidden) return
  if (pickerVisibilityTimer !== undefined) clearTimeout(pickerVisibilityTimer)
  pickerVisibilityTimer = window.setTimeout(() => {
    pickerVisibilityTimer = undefined
    if (awaitingSystemPicker.value) {
      abortPendingFolderPick('選擇未完成，請再試一次')
    }
  }, 12000)
}

function beginPickerWait() {
  pickerWentHidden = false
  clearPickerTimers()
  document.addEventListener('visibilitychange', onDocumentVisibilityChange)
  pickerSafetyTimer = window.setTimeout(() => {
    pickerSafetyTimer = undefined
    if (awaitingSystemPicker.value) {
      abortPendingFolderPick('選擇逾時，請再試一次')
    }
  }, 5 * 60 * 1000)
}

function endPickerWait() {
  clearPickerTimers()
  document.removeEventListener('visibilitychange', onDocumentVisibilityChange)
}

function promiseWithTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return promiseWithTimeoutCancellable(promise, ms, message).promise
}

function promiseWithTimeoutCancellable<T>(
  promise: Promise<T>,
  ms: number,
  message: string,
): { promise: Promise<T>; cancel: (overrideMsg?: string) => void } {
  let timer: ReturnType<typeof setTimeout> | undefined
  let rejectFn: ((e: Error) => void) | undefined
  const wrapped = new Promise<T>((resolve, reject) => {
    rejectFn = reject
    timer = setTimeout(() => reject(new Error(message)), ms)
    promise.then(
      (value) => {
        if (timer !== undefined) clearTimeout(timer)
        resolve(value)
      },
      (error) => {
        if (timer !== undefined) clearTimeout(timer)
        reject(error)
      },
    )
  })
  return {
    promise: wrapped,
    cancel: (overrideMsg?: string) => {
      if (timer !== undefined) clearTimeout(timer)
      timer = undefined
      rejectFn?.(new Error(overrideMsg ?? '已取消選擇'))
    },
  }
}

function abortPendingFolderPick(message?: string) {
  folderPickGeneration += 1
  activePickCancel?.(message ?? '已取消選擇')
  activePickCancel = null
  endPickerWait()
  awaitingSystemPicker.value = false
  if (message && !openingBusy.value) {
    status.value = formatReaderError(message)
  }
}

function cancelPickerWait() {
  abortPendingFolderPick('已取消選擇')
}

const canNavigateBooks = computed(
  () =>
    folderMultiZipMode.value &&
    folderSources.value.length > 1 &&
    readingActive.value &&
    currentSourceIndex.value >= 0,
)
const hasPrevBook = computed(() => canNavigateBooks.value && currentSourceIndex.value > 0)
const hasNextBook = computed(
  () =>
    canNavigateBooks.value &&
    currentSourceIndex.value >= 0 &&
    currentSourceIndex.value < folderSources.value.length - 1,
)

const localReaderDisplayTitle = computed(() =>
  pickLocalReaderTitle(
    readerTitle.value,
    folderSources.value,
    currentSourcePath.value,
    currentSourceIndex.value,
  ),
)

function isZipKind(kind?: 'zip' | 'folder') {
  return kind === 'zip'
}

function isContentUri(path: string) {
  return path.startsWith('content://')
}

function isZipPath(path: string): boolean {
  const lower = path.toLowerCase()
  return lower.endsWith('.zip') || lower.endsWith('.cbz') || lower.includes('.zip')
}

function titleForOpenedSource(displayTitle?: string, pagesTitle?: string): string {
  const fromList =
    displayTitle?.trim() ||
    (currentSourceIndex.value >= 0
      ? folderSources.value[currentSourceIndex.value]?.label?.trim()
      : '') ||
    folderSources.value.find((s) => s.path === currentSourcePath.value)?.label?.trim()
  if (fromList) return fromList
  const fallback = (pagesTitle ?? '').trim()
  if (fallback && !isGmSnapCacheTitle(fallback)) return fallback
  return '未命名篇章'
}

const folderPickerTitle = computed(() => decodeFolderDisplayLabel(folderLabel.value))

function stopProgressAnimation() {
  if (progressTimer !== undefined) {
    clearInterval(progressTimer)
    progressTimer = undefined
  }
}

function startIndeterminateProgress(label: string) {
  stopProgressAnimation()
  openProgressIndeterminate.value = true
  openProgressLabel.value = label
  openProgressPercent.value = 0
}

function startProgressAnimation(label: string, cap = 92) {
  stopProgressAnimation()
  openProgressIndeterminate.value = false
  openProgressLabel.value = label
  openProgressPercent.value = 8
  progressTimer = setInterval(() => {
    if (openProgressPercent.value < cap) {
      openProgressPercent.value = Math.min(cap, openProgressPercent.value + 1)
    }
  }, 320)
}

function setProgressDone() {
  stopProgressAnimation()
  openProgressIndeterminate.value = false
  openProgressPercent.value = 100
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

function pendingPageStyle(index: number): Record<string, string> | undefined {
  if (pageSrcMap.value.has(index) || failedIndices.value.has(index)) return undefined
  const h = estimatedPageHeight.value
  const minH = h > 0 ? h : Math.max(320, Math.floor(window.innerHeight * 0.72))
  return { minHeight: `${minH}px` }
}

function finishPositionRestore() {
  if (!isRestoringMode()) return
  stopRestoreSafetyTimer()
  endReaderRestore()
  endRestoreMode()
  restoreSaveBlocked = true
  saveReadingPosition()
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

function revokeAllBlobUrls() {
  for (const url of pageSrcMap.value.values()) {
    if (url.startsWith('blob:')) URL.revokeObjectURL(url)
  }
  pageSrcMap.value = new Map()
  failedIndices.value = new Set()
  clearReaderImageLoadProgress()
}

function clearReaderContent() {
  readerSessionId += 1
  restoreSaveBlocked = false
  resetLocalReaderViewportState()
  estimatedPageHeight.value = 0
  revokeAllBlobUrls()
  void closeLocalReaderZipSession()
  loadQueue = []
  activeLoads = 0
  clearPostSeekPrefetchTimer()
  observer?.disconnect()
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

function resetOpenQueue() {
  openGeneration += 1
  openQueue = Promise.resolve()
}

function forceAbortOpenQueue() {
  resetOpenQueue()
  restoreSaveBlocked = false
  resetLocalReaderViewportState()
  abortPendingFolderPick()
  releaseOpenBusyState()
}

function cancelFromFolderList() {
  forceAbortOpenQueue()
  clearReaderContent()
  folderMultiZipMode.value = false
  cancelFolderListMode()
  // 透過 ref 賦值，確保 toRefs 綁定的畫面立即切回閒置（避免僅改 store 卻不觸發重繪）
  pickingSource.value = false
  sourceListMode.value = false
  readingActive.value = false
  readerPages.value = []
  readerTitle.value = ''
  folderSources.value = []
  currentSourceIndex.value = -1
  currentSourcePath.value = ''
  status.value = ''
  exitFullscreen()
  scrollContainerRef.value?.scrollTo({ top: 0 })
}

function syncImageLoadProgress() {
  const total = readerPages.value.length
  if (!readingActive.value || total <= 0) {
    clearReaderImageLoadProgress()
    return
  }
  const loaded = pageSrcMap.value.size
  const failed = failedIndices.value.size
  const pending = total - loaded - failed
  updateReaderImageLoadProgress({
    active: pending > 0 || activeLoads > 0,
    loaded,
    total,
    failed,
  })
}

function updateScrollProgressBridge() {
  if (!readingActive.value) return
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

let postSeekPrefetchTimer: ReturnType<typeof setTimeout> | undefined

function clearPostSeekPrefetchTimer() {
  if (postSeekPrefetchTimer !== undefined) {
    clearTimeout(postSeekPrefetchTimer)
    postSeekPrefetchTimer = undefined
  }
}

function schedulePostSeekPrefetch(pageIndex: number, sessionId: number) {
  clearPostSeekPrefetchTimer()
  postSeekPrefetchTimer = setTimeout(() => {
    postSeekPrefetchTimer = undefined
    if (hasActiveScrollIntent()) return
    enqueueNeighborsInRadius(pageIndex, SCROLL_NEIGHBOR_RADIUS)
  }, 400)
}

/** 進度條鬆手：不先粗跳；目標頁解碼後只捲動一次 */
function seekByRatio(ratio: number) {
  if (isReaderProgressSeekDisabled() || isRestoringMode()) return
  const root = scrollContainerRef.value
  const total = readerPages.value.length
  if (!root || total <= 0) return

  const sessionId = beginSeekSession()
  const cp = ratioToPageCheckpoint(ratio, total)
  const hasSrc = (i: number) => pageSrcMap.value.has(i)

  clearPostSeekPrefetchTimer()
  setScrollIntent('seek', cp, sessionId)

  if (isPageImageDecoded(root, cp.pageIndex, hasSrc)) {
    commitScrollIntentOnce()
    clearScrollIntent()
    schedulePostSeekPrefetch(cp.pageIndex, sessionId)
    return
  }

  enqueuePage(cp.pageIndex)
}

function saveReadingPosition() {
  if (!readingActive.value || !currentSourcePath.value) return
  if (!canSaveReadingCheckpoint() || restoreSaveBlocked) return
  const root = scrollContainerRef.value
  const total = readerPages.value.length
  if (!root || total <= 0) return
  const snap = readCheckpointFromScroll(root, total, (i) => pageSrcMap.value.has(i))
  saveSourceReadPosition(
    currentSourcePath.value,
    snap.pageIndex,
    total,
    snap.scrollY,
    snap.offsetInPage,
    snap.offsetRatioInPage,
  )
}

function onReaderScroll() {
  onReaderScrollForChrome()
  if (!readingActive.value) return
  if (isRestoringMode() || hasActiveScrollIntent() || restoreScrollSuppress > 0) return
  if (isReaderScrollLocked()) return
  updateScrollProgressBridge()
  if (scrollSaveTimer !== undefined) clearTimeout(scrollSaveTimer)
  scrollSaveTimer = setTimeout(() => saveReadingPosition(), 500)
}

function resetAll() {
  cancelFromFolderList()
}

function closeReading() {
  saveReadingPosition()
  clearReaderContent()
  readingActive.value = false
  readerPages.value = []
  currentSourcePath.value = ''
  exitFullscreen()

  if (sourceListMode.value && folderSources.value.length > 0) {
    pickingSource.value = true
    readerTitle.value = folderPickerTitle.value || decodeFolderDisplayLabel(folderLabel.value)
    touchFolderPersist()
    scrollContainerRef.value?.scrollTo({ top: 0 })
    return
  }

  if (s.sessionKind === 'zip') {
    s.sessionKind = null
    s.currentSourceIndex = -1
    return
  }

  touchFolderPersist()
}

function maxConcurrentLoads() {
  return isZipPath(currentSourcePath.value) ? 2 : 4
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
  if (!readingActive.value) return
  if (!canEnqueuePage(index)) return
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
  const page = readerPages.value[index]
  if (!readingActive.value || !page || pageSrcMap.value.has(index)) return

  activeLoads += 1

  try {
    const bytes = await getLocalReaderImage(page.pageId)
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

function onPageImageLoad(index: number) {
  if (!readingActive.value || !currentSourcePath.value) return

  notifyPageImageReady(index)
  notePageHeight(index)

  const intent = getScrollIntent()
  if (!intent || intent.cp.pageIndex !== index) {
    if (!isRestoringMode() && !hasActiveScrollIntent()) updateScrollProgressBridge()
    return
  }

  requestAnimationFrame(() => {
    if (!commitScrollIntentOnce()) return

    const sessionId = intent.sessionId
    const center = intent.cp.pageIndex
    clearScrollIntent()

    if (intent.kind === 'restore' && isRestoringMode()) {
      finishPositionRestore()
      return
    }

    if (intent.kind === 'seek') {
      schedulePostSeekPrefetch(center, sessionId)
    }
  })
}

async function restoreReadingPosition() {
  stopRestoreSafetyTimer()
  const rec = currentSourcePath.value ? getSourceRecord(currentSourcePath.value) : null
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

async function activateReader() {
  if (!readingActive.value || readerPages.value.length === 0) return
  await nextTick()
  await nextTick()
  setupObserver()
  await restoreReadingPosition()
  updateScrollProgressBridge()
}

async function startReading(pages: LocalReaderPages, displayTitle?: string) {
  clearReaderContent()
  readerTitle.value = titleForOpenedSource(displayTitle, pages.title)
  readerPages.value = pages.pages
  pickingSource.value = false
  readingActive.value = true
  status.value = ''
  updateReaderImageLoadProgress({
    active: pages.pages.length > 0,
    loaded: 0,
    total: pages.pages.length,
  })
  if (currentSourcePath.value) {
    markSourceOpened(currentSourcePath.value, pages.pages.length)
  }
  touchFolderPersist()
}

async function loadPagesWithProgress(
  path: string,
  kind?: 'zip' | 'folder',
): Promise<LocalReaderPages> {
  const asZip = isZipKind(kind) || (!kind && isContentUri(path) && isZipPath(path))

  if (asZip && isContentUri(path)) {
    startIndeterminateProgress('正在複製 ZIP 到本機（檔案較大時請稍候）…')
    const cachedPath = await prepareLocalReaderZip(path)
    startProgressAnimation('正在解析 ZIP 頁面…', 95)
    const pages = await loadLocalReaderPages(cachedPath, 'zip')
    setProgressDone()
    return pages
  }

  if (asZip) {
    startProgressAnimation('正在解析 ZIP（檔案較大時請稍候）…', 95)
    const pages = await loadLocalReaderPages(path, 'zip')
    setProgressDone()
    return pages
  }

  startProgressAnimation('正在載入圖片列表…', 90)
  const pages = await loadLocalReaderPages(path, kind ?? 'folder')
  setProgressDone()
  return pages
}

function enqueueOpen(task: () => Promise<void>) {
  const gen = openGeneration
  openQueue = openQueue
    .then(async () => {
      if (gen !== openGeneration) return
      openingBusy.value = true
      status.value = ''
      try {
        await promiseWithTimeout(task(), 10 * 60 * 1000, '載入逾時，請重試')
      } catch (e) {
        if (gen === openGeneration) status.value = formatReaderError(e)
      } finally {
        stopProgressAnimation()
        if (gen === openGeneration) {
          releaseOpenBusyState()
          await nextTick()
          if (readingActive.value) await activateReader()
        } else if (openingBusy.value) {
          releaseOpenBusyState()
        }
      }
    })
    .catch((e) => {
      if (gen === openGeneration) status.value = formatReaderError(e)
      releaseOpenBusyState()
    })
  return openQueue
}

async function openSourceImpl(path: string, index: number, kind?: 'zip' | 'folder') {
  const resolvedKind = kind ?? folderSources.value[index]?.kind
  s.sessionKind = 'folder'
  currentSourceIndex.value = index
  currentSourcePath.value = path
  const label = folderSources.value[index]?.label
  const result = await loadPagesWithProgress(path, resolvedKind)
  await startReading(result, label)
}

function openSource(path: string, index: number, kind?: 'zip' | 'folder') {
  void enqueueOpen(() => openSourceImpl(path, index, kind))
}

async function openSourcePathImpl(path: string, kind: 'zip' | 'folder' = 'zip') {
  s.sessionKind = kind
  currentSourceIndex.value = -1
  currentSourcePath.value = path
  const result = await loadPagesWithProgress(path, kind)
  await startReading(result)
}

function openSourcePath(path: string, kind: 'zip' | 'folder' = 'zip') {
  void enqueueOpen(() => openSourcePathImpl(path, kind))
}

async function pickZipUri(): Promise<string | null> {
  const gen = ++folderPickGeneration
  awaitingSystemPicker.value = true
  status.value = '請在系統視窗選擇 ZIP 檔案'
  beginPickerWait()
  const pick = promiseWithTimeoutCancellable(
    pickLocalReaderZip(),
    5 * 60 * 1000,
    '選擇 ZIP 逾時，請重試',
  )
  activePickCancel = pick.cancel
  try {
    const uri = await pick.promise
    if (gen !== folderPickGeneration) return null
    return uri
  } catch (e) {
    if (gen === folderPickGeneration) status.value = formatReaderError(e)
    return null
  } finally {
    activePickCancel = null
    if (gen === folderPickGeneration) {
      endPickerWait()
      awaitingSystemPicker.value = false
      if (!openingBusy.value && status.value === '請在系統視窗選擇 ZIP 檔案') {
        status.value = ''
      }
    }
  }
}

async function pickFolderUri(): Promise<string | null> {
  const gen = ++folderPickGeneration
  awaitingSystemPicker.value = true
  status.value = '請在系統視窗選擇資料夾'
  beginPickerWait()
  const pick = promiseWithTimeoutCancellable(
    pickLocalReaderFolder(),
    5 * 60 * 1000,
    '選擇資料夾逾時，請重試',
  )
  activePickCancel = pick.cancel
  try {
    const uri = await pick.promise
    if (gen !== folderPickGeneration) return null
    return uri
  } catch (e) {
    if (gen === folderPickGeneration) status.value = formatReaderError(e)
    return null
  } finally {
    activePickCancel = null
    if (gen === folderPickGeneration) {
      endPickerWait()
      awaitingSystemPicker.value = false
      if (!openingBusy.value && status.value === '請在系統視窗選擇資料夾') {
        status.value = ''
      }
    }
  }
}

function openZipFile() {
  if (awaitingSystemPicker.value) return
  if (openingBusy.value) {
    releaseOpenBusyState()
    resetOpenQueue()
  }
  void (async () => {
    try {
      const selected = await pickZipUri()
      if (!selected) return
      folderMultiZipMode.value = false
      sourceListMode.value = false
      folderSources.value = []
      s.folderTreeUri = ''
      resetOpenQueue()
      await enqueueOpen(() => openSourcePathImpl(selected, 'zip'))
    } catch (e) {
      status.value = formatReaderError(e)
    } finally {
      awaitingSystemPicker.value = false
      endPickerWait()
    }
  })()
}

function openFolder() {
  if (awaitingSystemPicker.value) return
  if (openingBusy.value) {
    releaseOpenBusyState()
    resetOpenQueue()
  }
  void (async () => {
    try {
      const selected = await pickFolderUri()
      if (!selected) return
      resetOpenQueue()
      await enqueueOpen(async () => {
        s.sessionKind = 'folder'
        s.folderTreeUri = selected
        loadFolderSourceProgress(selected)
        startIndeterminateProgress('正在掃描資料夾…')
        const sources = await promiseWithTimeout(
          listLocalReaderSources(selected),
          5 * 60 * 1000,
          '掃描資料夾逾時，請重試',
        )
        if (sources.length === 0) {
          status.value = '此資料夾內沒有可閱讀的 ZIP 或圖片子資料夾'
          return
        }
        clearReaderContent()
        readingActive.value = false
        readerPages.value = []
        folderMultiZipMode.value = true
        sourceListMode.value = true
        folderLabel.value = decodeFolderDisplayLabel(selected)
        readerTitle.value = folderLabel.value
        pickingSource.value = true
        folderSources.value = sources
        currentSourceIndex.value = -1
        currentSourcePath.value = ''
        status.value = ''
        touchFolderPersist()
      })
    } catch (e) {
      status.value = formatReaderError(e)
    } finally {
      awaitingSystemPicker.value = false
      endPickerWait()
    }
  })()
}

function goToAdjacentSource(delta: number) {
  saveReadingPosition()
  const nextIndex = currentSourceIndex.value + delta
  const source = folderSources.value[nextIndex]
  if (!source) return
  void openSource(source.path, nextIndex, source.kind)
}

function clearSourceRecord(path: string) {
  if (!hasSourceReadRecord(path)) return
  clearSourceReadRecord(path)
}

function resumeIfNeeded() {
  if (!openingBusy.value && readingActive.value && readerPages.value.length > 0) {
    void activateReader()
  }
}

watch(
  readingActive,
  (active) => {
    if (active) {
      registerReaderScrollControl({
        seek: seekByRatio,
        savePosition: () => {
          saveReadingPosition()
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
  if (localReadSession.sourceListMode && localReadSession.folderSources.length > 0) {
    folderMultiZipMode.value = true
  }
  if (folderLabel.value) {
    folderLabel.value = decodeFolderDisplayLabel(folderLabel.value)
  }
  if (pickingSource.value && !readingActive.value) {
    readerTitle.value = folderPickerTitle.value
  }
  resumeIfNeeded()
})
onActivated(() => resumeIfNeeded())

/** 切換到其他次導覽（如影片播放）時關閉 Teleport 遮罩，避免蓋住整個 App */
onDeactivated(() => {
  if (awaitingSystemPicker.value) {
    abortPendingFolderPick('已切換分頁，選擇已取消')
  } else if (openingBusy.value) {
    releaseOpenBusyState()
    status.value = ''
  }
})

onBeforeUnmount(() => {
  endPickerWait()
  forceAbortOpenQueue()
  if (scrollSaveTimer !== undefined) clearTimeout(scrollSaveTimer)
  stopRestoreSafetyTimer()
  saveReadingPosition()
  unregisterReaderScrollControl()
  stopProgressAnimation()
  touchFolderPersist()
  observer?.disconnect()
})
</script>

<template>
  <div class="local-read-root">
  <Teleport to="body">
    <div
      v-if="awaitingSystemPicker"
      class="open-overlay open-overlay--picker"
      @click.stop
      @touchmove.stop.prevent
    >
      <div class="open-overlay-card">
        <p class="open-overlay-title">{{ status || '請在系統視窗選擇…' }}</p>
        <p class="open-overlay-sub">若已選完仍停在此畫面，請點取消後再試一次</p>
        <button type="button" class="reader-btn open-overlay-cancel" @click="cancelPickerWait">取消</button>
      </div>
    </div>
    <div
      v-else-if="openingBusy"
      class="open-overlay"
      @click.stop
      @touchmove.stop.prevent
    >
      <div class="open-overlay-card">
        <p class="open-overlay-title">{{ openProgressLabel || '載入中…' }}</p>
        <div
          class="open-progress-track"
          :class="{ 'open-progress-track--indeterminate': openProgressIndeterminate }"
        >
          <div
            class="open-progress-bar"
            :class="{ 'open-progress-bar--indeterminate': openProgressIndeterminate }"
            :style="openProgressIndeterminate ? undefined : { width: `${openProgressPercent}%` }"
          />
        </div>
        <p v-if="!openProgressIndeterminate" class="open-overlay-hint">{{ openProgressPercent }}%</p>
        <p v-else class="open-overlay-hint">處理中，請稍候…</p>
        <button type="button" class="reader-btn open-overlay-cancel" @click="resetAll">取消</button>
      </div>
    </div>
  </Teleport>

  <div v-if="pickingSource" class="reader-shell">
    <div class="reader-header">
      <div class="reader-header-title">
        {{ folderPickerTitle }}
        <span class="page-meta">請選擇要閱讀的篇章</span>
      </div>
      <div class="reader-header-actions">
        <button type="button" class="reader-btn" @click="resetAll">取消</button>
      </div>
    </div>
    <p v-if="status && !openingBusy && !awaitingSystemPicker" class="reader-ph err">{{ status }}</p>
    <p v-if="awaitingSystemPicker" class="reader-ph picker-hint">{{ status }}</p>
    <div class="reader-source-list">
      <div
        v-for="(source, index) in folderSources"
        :key="source.path"
        class="reader-source-item"
      >
        <button
          type="button"
          class="reader-source-main"
          :disabled="openingBusy"
          @click="openSource(source.path, index, source.kind)"
        >
          <span class="source-btn-main">
            <span v-if="getSourceRecord(source.path).opened" class="opened-dot" title="已開啟過">●</span>
            {{ source.label }}
            <span class="kind-tag">{{ source.kind === 'zip' ? 'ZIP' : '資料夾' }}</span>
          </span>
          <span v-if="formatSourceProgressLabel(source.path)" class="source-progress">
            {{ formatSourceProgressLabel(source.path) }}
          </span>
        </button>
        <button
          type="button"
          class="reader-source-clear"
          :disabled="openingBusy || !hasSourceReadRecord(source.path)"
          @click.stop="clearSourceRecord(source.path)"
        >
          清除紀錄
        </button>
      </div>
    </div>
  </div>

  <div v-else-if="!readingActive" class="reader-shell">
    <div class="reader-idle">
      <p>從本地 ZIP 或資料夾開啟漫畫</p>
      <div class="idle-actions">
        <button
          type="button"
          class="reader-btn reader-btn--primary"
          :disabled="openingBusy || awaitingSystemPicker"
          @click="openZipFile"
        >
          開啟 ZIP 檔
        </button>
        <button
          type="button"
          class="reader-btn"
          :disabled="openingBusy || awaitingSystemPicker"
          @click="openFolder"
        >
          開啟資料夾
        </button>
      </div>
      <p class="folder-hint">
        資料夾模式會列出其中的 ZIP 或子資料夾，點選後開始閱讀（僅一本也會顯示列表）。
      </p>

      <div class="stream-home-section">
        <p class="stream-home-heading">PC 串流閱讀</p>
        <p class="folder-hint stream-home-hint">
          從遠端管理勾選 ZIP/CBZ 後「串流 → 串流閱讀」
        </p>
        <div class="idle-actions stream-home-actions">
          <button
            type="button"
            class="reader-btn reader-btn--primary"
            :disabled="!streamCurrentJob || streamOpeningBusy"
            @click="startStreamReadFromHome"
          >
            開始串流閱讀
          </button>
        </div>
        <p v-if="streamOpeningBusy" class="stream-home-status">正在載入串流漫畫…</p>
        <p v-else-if="streamStatus" class="reader-ph err stream-home-status">{{ streamStatus }}</p>
      </div>

      <p v-if="status && !openingBusy && !awaitingSystemPicker" class="reader-ph err">{{ status }}</p>
      <p v-if="awaitingSystemPicker" class="reader-ph picker-hint">{{ status }}</p>
    </div>
  </div>

  <div v-else :class="['reader-shell', { 'reader-shell--fullscreen': isFullscreen }]">
    <div
      v-if="isFullscreen"
      class="reader-top-chrome"
      :class="{ 'reader-top-chrome--hidden': !chromeVisible }"
    >
      <div class="reader-top-title" :title="localReaderDisplayTitle">
        {{ localReaderDisplayTitle }}
      </div>
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
          <template v-if="canNavigateBooks">
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
.local-read-root {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
}
.page-meta {
  font-weight: 400;
  font-size: 12px;
  opacity: 0.7;
  margin-left: 6px;
}
.idle-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: center;
}
.folder-hint {
  font-size: 11px;
  color: #888;
  max-width: 320px;
  line-height: 1.4;
}
.stream-home-section {
  width: 100%;
  max-width: 340px;
  margin-top: 20px;
  padding-top: 16px;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
  text-align: center;
}
.stream-home-heading {
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 600;
  color: #bbb;
}
.stream-home-hint {
  margin: 0 auto 12px;
}
.stream-home-actions {
  margin-bottom: 4px;
}
.stream-home-status {
  margin: 8px 0 0;
  font-size: 12px;
}
.kind-tag {
  opacity: 0.6;
  margin-left: 6px;
  font-size: 10px;
}
.source-btn-main {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 4px;
  width: 100%;
}
.opened-dot {
  color: #6eb5ff;
  font-size: 10px;
  line-height: 1;
}
.source-progress {
  display: block;
  width: 100%;
  font-size: 11px;
  color: #9ab;
  opacity: 0.9;
  line-height: 1.3;
}
.reader-ph.err {
  color: #e88;
}
</style>

<style>
.open-overlay {
  position: fixed;
  inset: 0;
  z-index: 200000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.72);
  padding: 24px;
}

.open-overlay--picker {
  background: rgba(0, 0, 0, 0.45);
}

.open-overlay-card {
  width: min(320px, 100%);
  padding: 20px 18px;
  border-radius: 10px;
  background: #1e1e1e;
  border: 1px solid #444;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
}

.open-overlay-title {
  margin: 0 0 14px;
  font-size: 13px;
  color: #eee;
  text-align: center;
  line-height: 1.45;
}

.open-progress-track {
  height: 8px;
  border-radius: 4px;
  background: #333;
  overflow: hidden;
}

.open-progress-bar {
  height: 100%;
  border-radius: 4px;
  background: linear-gradient(90deg, #3d6ef5, #5b8fff);
  transition: width 0.25s ease;
}

.open-overlay-hint {
  margin: 10px 0 0;
  font-size: 12px;
  color: #8ab4f8;
  text-align: center;
}

.open-overlay-sub {
  margin: 8px 0 0;
  font-size: 11px;
  color: #888;
  text-align: center;
  line-height: 1.4;
}

.reader-page--pending {
  background: #0d0d0d;
}

.open-overlay-cancel {
  display: block;
  width: 100%;
  margin-top: 14px;
}

.open-progress-track--indeterminate {
  overflow: hidden;
}

.open-progress-bar--indeterminate {
  width: 40% !important;
  animation: open-progress-slide 1.2s ease-in-out infinite;
}

@keyframes open-progress-slide {
  0% {
    transform: translateX(-120%);
  }
  100% {
    transform: translateX(320%);
  }
}

.picker-hint {
  color: #8ab4f8;
  text-align: center;
  padding: 12px;
}
</style>
