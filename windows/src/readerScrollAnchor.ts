import { getPageHeight, getPageTopInScroller, seekToContentRatio } from './readerProgressMath.ts'
import type { SourceReadRecord } from './localReadStore.ts'

/** 佔位或尚未載入的頁面高度上限（真實漫畫頁通常遠大於此） */
const LAYOUT_READY_MIN_PAGE_HEIGHT = 120

export function isPageLayoutReady(root: HTMLElement, pageIndex: number): boolean {
  return getPageHeight(root, pageIndex) >= LAYOUT_READY_MIN_PAGE_HEIGHT
}

/** 依 Perfect Viewer 單斷點還原：優先 scrollY；版面未穩定時先用頁序比例定位 */
export function applyReadingRecord(
  root: HTMLElement,
  totalPages: number,
  rec: Pick<SourceReadRecord, 'readPage' | 'scrollY'>,
): void {
  if (totalPages <= 0) return

  const pageIndex = Math.min(Math.max(0, rec.readPage), totalPages - 1)
  const targetY = Math.max(0, rec.scrollY)

  if (totalPages === 1) {
    const h = getPageHeight(root, 0)
    root.scrollTop = h > 0 ? Math.min(targetY, h) : 0
    return
  }

  const pageReady = isPageLayoutReady(root, pageIndex)

  if (!pageReady) {
    const ratio =
      totalPages <= 1 ? 0 : Math.min(1, pageIndex / Math.max(1, totalPages - 1))
    seekToContentRatio(root, totalPages, ratio)
    return
  }

  if (targetY > 0) {
    root.scrollTop = targetY
    return
  }

  root.scrollTop = getPageTopInScroller(root, pageIndex)
}

export function shouldReanchorForPageLoad(
  anchorPageIndex: number,
  loadedPageIndex: number,
  radius = 3,
): boolean {
  return Math.abs(loadedPageIndex - anchorPageIndex) <= radius
}
