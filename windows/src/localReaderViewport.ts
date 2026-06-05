/**
 * 本地閱讀捲動控制：斷點還原 / 進度條跳轉 / 手動滑動 互斥。
 * 程式化跳轉：**不粗跳**，等目標頁圖片解碼後只改 scrollTop 一次。
 */

import type { SourceReadRecord } from './localReadStore'
import {
  computeVisiblePageIndex,
  getPageElement,
  getPageHeight,
  getPageTopInScroller,
  type PageCheckpoint,
  ratioToPageCheckpoint,
  recordToPageCheckpoint,
} from './readerProgressMath'

export type { PageCheckpoint }
export { ratioToPageCheckpoint, recordToPageCheckpoint }

export const RESTORE_NEIGHBOR_RADIUS = 3
export const SCROLL_NEIGHBOR_RADIUS = 3
export const MIN_PAGE_HEIGHT_FOR_RATIO = 80

export type LocalReaderMode = 'reading' | 'restoring'
export type ScrollIntentKind = 'seek' | 'restore'

export type ScrollIntent = {
  kind: ScrollIntentKind
  cp: PageCheckpoint
  sessionId: number
}

let mode: LocalReaderMode = 'reading'
let restoreBookmarkPage = -1
let scrollIntent: ScrollIntent | null = null
let seekSessionId = 0

const pageImageReadyWaiters = new Map<number, Set<() => void>>()

export function getLocalReaderMode(): LocalReaderMode {
  return mode
}

export function isRestoringMode(): boolean {
  return mode === 'restoring'
}

export function hasActiveScrollIntent(): boolean {
  return scrollIntent !== null
}

export function getScrollIntent(): ScrollIntent | null {
  return scrollIntent
}

export function setScrollIntent(kind: ScrollIntentKind, cp: PageCheckpoint, sessionId: number) {
  scrollIntent = { kind, cp, sessionId }
}

export function clearScrollIntent() {
  scrollIntent = null
}

export function canSaveCheckpoint(): boolean {
  return mode === 'reading' && scrollIntent === null
}

export function canSaveReadingCheckpoint(): boolean {
  return canSaveCheckpoint()
}

export function canObserverEnqueuePages(): boolean {
  return mode !== 'restoring' && scrollIntent === null
}

export function canEnqueuePage(index: number): boolean {
  if (mode !== 'restoring' || restoreBookmarkPage < 0) return true
  return Math.abs(index - restoreBookmarkPage) <= RESTORE_NEIGHBOR_RADIUS
}

export function beginRestoreMode(bookmarkPage: number) {
  mode = 'restoring'
  restoreBookmarkPage = bookmarkPage
}

export function endRestoreMode() {
  mode = 'reading'
  restoreBookmarkPage = -1
}

export function getRestoreBookmarkPage(): number {
  return restoreBookmarkPage
}

export function resetLocalReaderViewportState() {
  mode = 'reading'
  restoreBookmarkPage = -1
  scrollIntent = null
  seekSessionId = 0
  pageImageReadyWaiters.clear()
}

export function beginSeekSession(): number {
  seekSessionId += 1
  return seekSessionId
}

export function getSeekSessionId(): number {
  return seekSessionId
}

export function isSeekSessionActive(sessionId: number): boolean {
  return sessionId === seekSessionId && sessionId > 0
}

export function isStartCheckpoint(rec: SourceReadRecord): boolean {
  return (
    rec.readPage === 0 &&
    (rec.scrollY ?? 0) === 0 &&
    (rec.offsetInPage ?? 0) === 0 &&
    (rec.offsetRatioInPage ?? 0) === 0
  )
}

export function defaultEstimatedPageHeight(current: number): number {
  return current > 0 ? current : Math.max(320, Math.floor(window.innerHeight * 0.72))
}

export function isPageImageDecoded(
  root: HTMLElement,
  pageIndex: number,
  hasPageSrc: (index: number) => boolean,
): boolean {
  if (!hasPageSrc(pageIndex)) return false
  const pageEl = getPageElement(root, pageIndex)
  const img = pageEl?.querySelector('img')
  if (!(img instanceof HTMLImageElement)) return false
  return img.complete && img.naturalHeight > 0
}

/** 唯一允許的程式化捲動：目標頁已解碼後，依頁頂 + 頁內比例 */
export function commitPageCheckpoint(
  root: HTMLElement,
  cp: PageCheckpoint,
  hasPageSrc: (index: number) => boolean,
): boolean {
  if (!hasPageSrc(cp.pageIndex)) return false
  const pageTop = getPageTopInScroller(root, cp.pageIndex)
  const pageH = getPageHeight(root, cp.pageIndex)
  if (pageH < MIN_PAGE_HEIGHT_FOR_RATIO) return false
  const ratio = Math.min(1, Math.max(0, cp.offsetRatioInPage))
  const offset = ratio * pageH
  root.scrollTop = pageTop + Math.min(offset, Math.max(0, pageH - 1))
  return true
}

export function notifyPageImageReady(index: number) {
  const waiters = pageImageReadyWaiters.get(index)
  if (!waiters) return
  pageImageReadyWaiters.delete(index)
  for (const fn of waiters) fn()
}

export function waitForPageImageReady(
  pageIndex: number,
  isSessionAlive: () => boolean,
  timeoutMs = 45_000,
): Promise<boolean> {
  return new Promise((resolve) => {
    if (!isSessionAlive()) {
      resolve(false)
      return
    }
    const timer = window.setTimeout(() => {
      cleanup()
      resolve(false)
    }, timeoutMs)

    const finish = (ok: boolean) => {
      cleanup()
      resolve(ok && isSessionAlive())
    }

    const cleanup = () => {
      clearTimeout(timer)
      const set = pageImageReadyWaiters.get(pageIndex)
      if (set) {
        set.delete(onReady)
        if (set.size === 0) pageImageReadyWaiters.delete(pageIndex)
      }
    }

    const onReady = () => finish(true)

    let set = pageImageReadyWaiters.get(pageIndex)
    if (!set) {
      set = new Set()
      pageImageReadyWaiters.set(pageIndex, set)
    }
    set.add(onReady)
  })
}

export function readCheckpointFromScroll(
  root: HTMLElement,
  totalPages: number,
  hasPageSrc: (index: number) => boolean,
): PageCheckpoint & { scrollY: number; offsetInPage: number } {
  const pageIndex = Math.min(totalPages - 1, Math.max(0, computeVisiblePageIndex(root)))
  const pageTop = getPageTopInScroller(root, pageIndex)
  const offsetInPage = Math.max(0, root.scrollTop - pageTop)
  const pageH = getPageHeight(root, pageIndex)
  const offsetRatioInPage =
    pageH >= MIN_PAGE_HEIGHT_FOR_RATIO
      ? Math.min(1, offsetInPage / pageH)
      : 0
  return {
    pageIndex,
    offsetRatioInPage,
    scrollY: root.scrollTop,
    offsetInPage,
  }
}
