/**
 * 長篇漫畫閱讀：避免 placeholder／圖片高度變化造成捲動回彈。
 * - 每頁鎖定 placeholder minHeight（不因全域估算同步改寫所有 pending 頁）
 * - 圖片 decode 前保持 minHeight（避免 src 寫入後高度塌成 0）
 * - 視窗上方頁面高度變化時補償 scrollTop
 */

import { getPageElement, getPageTopInScroller } from './readerProgressMath'

export type PagePlaceholderState = {
  lockedHeights: Map<number, number>
  decoded: Set<number>
}

export function createPagePlaceholderState(): PagePlaceholderState {
  return {
    lockedHeights: new Map(),
    decoded: new Set(),
  }
}

export function clearPagePlaceholderState(state: PagePlaceholderState) {
  state.lockedHeights.clear()
  state.decoded.clear()
}

export function defaultPlaceholderPx(estimatedPageHeight: number): number {
  return estimatedPageHeight > 0
    ? estimatedPageHeight
    : Math.max(320, Math.floor(window.innerHeight * 0.72))
}

export function lockPagePlaceholder(
  state: PagePlaceholderState,
  index: number,
  estimatedPageHeight: number,
): number {
  const existing = state.lockedHeights.get(index)
  if (existing != null && existing > 0) return existing
  const h = defaultPlaceholderPx(estimatedPageHeight)
  state.lockedHeights.set(index, h)
  return h
}

/** 未 decode 前一律保留 minHeight（含已有 blob src 尚未 @load） */
export function pageSlotMinHeightStyle(
  state: PagePlaceholderState,
  index: number,
  opts: {
    hasFailed: boolean
    estimatedPageHeight: number
    /** 觸發 Vue 在 decode 後重算 style */
    decodedTick: number
  },
): Record<string, string> | undefined {
  void opts.decodedTick
  if (opts.hasFailed) return undefined
  if (state.decoded.has(index)) return undefined
  const h = lockPagePlaceholder(state, index, opts.estimatedPageHeight)
  return { minHeight: `${h}px` }
}

export function markPageDecoded(state: PagePlaceholderState, index: number) {
  state.decoded.add(index)
  state.lockedHeights.delete(index)
}

export type PageHeightStabilizer = {
  observe: (el: HTMLElement) => void
  disconnect: () => void
  reset: () => void
  setEnabled: (enabled: boolean) => void
}

/**
 * 監聽各頁高度；若該頁完全在視窗上方，將 scrollTop 加上高度差以維持畫面內容。
 */
export function createPageHeightStabilizer(opts: {
  getRoot: () => HTMLElement | null
  /** false 時略過補償（還原／seek／程式化捲動） */
  shouldCompensate: () => boolean
  onCompensateStart?: () => void
  onCompensateEnd?: () => void
}): PageHeightStabilizer {
  const lastHeights = new Map<number, number>()
  let enabled = true

  const ro = new ResizeObserver((entries) => {
    const root = opts.getRoot()
    if (!root) return

    const allowCompensate = enabled && opts.shouldCompensate()
    const scrollTop = root.scrollTop
    let totalDelta = 0

    for (const entry of entries) {
      const el = entry.target
      if (!(el instanceof HTMLElement)) continue
      const index = Number(el.dataset.index)
      if (Number.isNaN(index)) continue

      const newH = el.offsetHeight || entry.contentRect.height
      if (newH <= 0) continue
      const oldH = lastHeights.get(index)
      // seek／還原期間仍更新紀錄，避免結束後一次補上巨大 delta
      lastHeights.set(index, newH)
      if (!allowCompensate || oldH == null || oldH === newH) continue

      const pageTop = getPageTopInScroller(root, index)
      // 高度變化前該頁底邊仍在視窗上方 → 需補償
      if (pageTop + oldH <= scrollTop + 1) {
        totalDelta += newH - oldH
      }
    }

    if (totalDelta === 0) return
    opts.onCompensateStart?.()
    root.scrollTop = scrollTop + totalDelta
    requestAnimationFrame(() => {
      opts.onCompensateEnd?.()
    })
  })

  return {
    observe(el: HTMLElement) {
      const h = el.offsetHeight
      const index = Number(el.dataset.index)
      if (!Number.isNaN(index) && h > 0) lastHeights.set(index, h)
      ro.observe(el)
    },
    disconnect() {
      ro.disconnect()
      lastHeights.clear()
    },
    reset() {
      lastHeights.clear()
    },
    setEnabled(next: boolean) {
      enabled = next
    },
  }
}

export function capturePageHeightSnap(root: HTMLElement, pageIndex: number) {
  const el = getPageElement(root, pageIndex)
  if (!el) return null
  const pageTop = getPageTopInScroller(root, pageIndex)
  const height = el.offsetHeight || el.getBoundingClientRect().height
  const scrollTop = root.scrollTop
  return {
    scrollTop,
    height,
    pageTop,
    above: pageTop + height <= scrollTop + 1,
  }
}
