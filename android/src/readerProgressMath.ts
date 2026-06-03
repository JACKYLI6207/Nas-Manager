/** 以「第幾頁 + 頁內捲動」計算 0–1 進度，避免 scrollHeight 未載完時比例失真 */

/** 書籤座標：第幾頁 + 該頁內 0–1 位置（與 scrollHeight 無關） */
export type PageCheckpoint = {
  pageIndex: number
  offsetRatioInPage: number
}

export function ratioToPageCheckpoint(ratio: number, totalPages: number): PageCheckpoint {
  if (totalPages <= 0) return { pageIndex: 0, offsetRatioInPage: 0 }
  if (totalPages === 1) {
    return { pageIndex: 0, offsetRatioInPage: Math.min(1, Math.max(0, ratio)) }
  }
  const clamped = Math.min(1, Math.max(0, ratio))
  const floatIdx = clamped * (totalPages - 1)
  const pageIndex = Math.min(totalPages - 1, Math.floor(floatIdx))
  const offsetRatioInPage = floatIdx - pageIndex
  return { pageIndex, offsetRatioInPage }
}

export function recordToPageCheckpoint(
  rec: {
    readPage: number
    scrollY?: number
    offsetInPage?: number
    offsetRatioInPage?: number
  },
  estimatedPageHeight: number,
): PageCheckpoint {
  const pageIndex = Math.max(0, rec.readPage)
  let offsetRatioInPage = Math.min(1, Math.max(0, rec.offsetRatioInPage ?? 0))
  const pageH = estimatedPageHeight > 0 ? estimatedPageHeight : 1
  if (offsetRatioInPage <= 0) {
    const stored = Math.max(0, rec.offsetInPage ?? 0)
    if (stored > 0) offsetRatioInPage = Math.min(1, stored / pageH)
  }
  if (offsetRatioInPage <= 0) {
    const targetY = Math.max(0, rec.scrollY ?? 0)
    const approxTop = pageIndex * pageH
    if (targetY > approxTop + 8) {
      offsetRatioInPage = Math.min(1, (targetY - approxTop) / pageH)
    }
  }
  return { pageIndex, offsetRatioInPage }
}

export function getPageElement(root: HTMLElement, pageIndex: number): HTMLElement | null {
  return root.querySelector<HTMLElement>(`[data-index="${pageIndex}"]`)
}

export function getPageTopInScroller(root: HTMLElement, pageIndex: number): number {
  const el = getPageElement(root, pageIndex)
  if (!el) return 0
  const rootRect = root.getBoundingClientRect()
  const elRect = el.getBoundingClientRect()
  return root.scrollTop + (elRect.top - rootRect.top)
}

export function getPageHeight(root: HTMLElement, pageIndex: number): number {
  const el = getPageElement(root, pageIndex)
  if (!el) return 1
  const img = el.querySelector('img')
  if (img instanceof HTMLImageElement && img.complete) {
    const ih = img.offsetHeight || img.getBoundingClientRect().height
    if (ih > 0) return ih
  }
  const h = el.offsetHeight || el.getBoundingClientRect().height
  return h > 0 ? h : 1
}

export function inferPageHeightFromRecord(rec: {
  readPage: number
  scrollY?: number
  offsetRatioInPage?: number
  offsetInPage?: number
}): number {
  const pageIndex = Math.max(0, rec.readPage)
  const ratio = Math.min(1, Math.max(0, rec.offsetRatioInPage ?? 0))
  const scrollY = Math.max(0, rec.scrollY ?? 0)
  if (scrollY > 0 && pageIndex + ratio >= 0.01) {
    return Math.max(320, Math.round(scrollY / (pageIndex + ratio)))
  }
  const off = Math.max(0, rec.offsetInPage ?? 0)
  if (off > 0 && ratio > 0.01) {
    return Math.max(320, Math.round(off / ratio))
  }
  return 0
}

export function computeVisiblePageIndex(root: HTMLElement): number {
  const rootRect = root.getBoundingClientRect()
  const viewportBottom = rootRect.bottom
  let best = 0
  let bestArea = -1
  const nodes = root.querySelectorAll<HTMLElement>('[data-index]')
  for (const node of nodes) {
    const idx = Number(node.dataset.index)
    if (Number.isNaN(idx)) continue
    const rect = node.getBoundingClientRect()
    const visibleTop = Math.max(rect.top, rootRect.top)
    const visibleBottom = Math.min(rect.bottom, viewportBottom)
    const area = Math.max(0, visibleBottom - visibleTop)
    if (area > bestArea) {
      bestArea = area
      best = idx
    }
  }
  return best
}

/** 0–1：在 totalPages 個篇章中的閱讀位置（含長圖頁內偏移） */
export function computeContentRatio(
  root: HTMLElement,
  totalPages: number,
  pageIndex = computeVisiblePageIndex(root),
): number {
  if (totalPages <= 0) return 0
  if (totalPages === 1) {
    const h = getPageHeight(root, 0)
    return h > 0 ? Math.min(1, Math.max(0, root.scrollTop / h)) : 0
  }
  const pageTop = getPageTopInScroller(root, pageIndex)
  const offsetInPage = Math.max(0, root.scrollTop - pageTop)
  const pageH = getPageHeight(root, pageIndex)
  const withinPage = Math.min(1, offsetInPage / pageH)
  return Math.min(1, Math.max(0, (pageIndex + withinPage) / (totalPages - 1)))
}

export function contentRatioToPageLabel(ratio: number, totalPages: number): {
  pageIndex: number
  currentPage: number
  pageLabel: string
} {
  if (totalPages <= 0) {
    return { pageIndex: 0, currentPage: 0, pageLabel: '' }
  }
  if (totalPages === 1) {
    return { pageIndex: 0, currentPage: 1, pageLabel: '1/1頁' }
  }
  const clamped = Math.min(1, Math.max(0, ratio))
  const floatIdx = clamped * (totalPages - 1)
  const pageIndex = Math.min(totalPages - 1, Math.floor(floatIdx))
  const currentPage = pageIndex + 1
  return { pageIndex, currentPage, pageLabel: `${currentPage}/${totalPages}頁` }
}

/** 依內容比例捲動；回傳目標 pageIndex 供預載 */
export function seekToContentRatio(
  root: HTMLElement,
  totalPages: number,
  ratio: number,
): number {
  if (totalPages <= 0) return 0
  const clamped = Math.min(1, Math.max(0, ratio))

  if (totalPages === 1) {
    const h = getPageHeight(root, 0)
    root.scrollTop = clamped * h
    return 0
  }

  const floatIdx = clamped * (totalPages - 1)
  const pageIndex = Math.min(totalPages - 1, Math.floor(floatIdx))
  const withinPage = floatIdx - pageIndex
  const pageTop = getPageTopInScroller(root, pageIndex)
  const pageH = getPageHeight(root, pageIndex)
  root.scrollTop = pageTop + withinPage * pageH
  return pageIndex
}
