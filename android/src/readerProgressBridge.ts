import { reactive, ref } from 'vue'
import { contentRatioToPageLabel } from './readerProgressMath'

export type ReaderScrollProgress = {
  active: boolean
  /** 0–1，僅供進度條顯示（頁面+頁內比例，與文字一致） */
  ratio: number
  pageLabel: string
  currentPage: number
  totalPages: number
}

export const readerScrollProgress = reactive<ReaderScrollProgress>({
  active: false,
  ratio: 0,
  pageLabel: '',
  currentPage: 0,
  totalPages: 0,
})

/** 本地閱讀：圖片預載進度（顯示於底部進度條，格式 loaded/total） */
export const readerImageLoadProgress = reactive({
  active: false,
  loaded: 0,
  total: 0,
  failed: 0,
})

export const readerSeeking = ref(false)
export const readerRestoring = ref(false)

let seekHandler: ((ratio: number) => void) | null = null
let savePositionHandler: (() => void) | null = null

export function isReaderScrollLocked(): boolean {
  return readerSeeking.value || readerRestoring.value
}

/** 還原斷點進行中時暫禁進度條拖曳，避免與定位衝突 */
export function isReaderProgressSeekDisabled(): boolean {
  return readerRestoring.value
}

export function beginReaderSeek() {
  readerSeeking.value = true
}

export function endReaderSeek() {
  readerSeeking.value = false
}

export function beginReaderRestore() {
  readerRestoring.value = true
}

export function endReaderRestore() {
  readerRestoring.value = false
}

export function registerReaderScrollControl(handlers: {
  seek: (ratio: number) => void
  savePosition?: () => void
}) {
  seekHandler = handlers.seek
  savePositionHandler = handlers.savePosition ?? null
}

export function unregisterReaderScrollControl() {
  seekHandler = null
  savePositionHandler = null
  readerSeeking.value = false
  readerRestoring.value = false
  readerScrollProgress.active = false
  readerScrollProgress.ratio = 0
  readerScrollProgress.pageLabel = ''
  readerScrollProgress.currentPage = 0
  readerScrollProgress.totalPages = 0
  clearReaderImageLoadProgress()
}

export function updateReaderImageLoadProgress(patch: Partial<typeof readerImageLoadProgress>) {
  Object.assign(readerImageLoadProgress, patch)
}

export function clearReaderImageLoadProgress() {
  readerImageLoadProgress.active = false
  readerImageLoadProgress.loaded = 0
  readerImageLoadProgress.total = 0
  readerImageLoadProgress.failed = 0
}

export function updateReaderScrollProgress(patch: Partial<ReaderScrollProgress>) {
  if (readerSeeking.value) return
  Object.assign(readerScrollProgress, patch)
}

export function setReaderSeekPreview(ratio: number) {
  const clamped = Math.min(1, Math.max(0, ratio))
  const total = readerScrollProgress.totalPages
  const { currentPage, pageLabel } = contentRatioToPageLabel(clamped, total)
  readerScrollProgress.ratio = clamped
  readerScrollProgress.currentPage = currentPage
  readerScrollProgress.pageLabel = pageLabel
}

export function seekReaderByRatio(ratio: number) {
  seekHandler?.(Math.min(1, Math.max(0, ratio)))
}

export function notifyReaderSeekEnd() {
  savePositionHandler?.()
}
