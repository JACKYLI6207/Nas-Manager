import { onBeforeUnmount, ref, watch } from 'vue'
import { readerFullscreenActive } from './useReaderFullscreen'

/** 手指位移超過此值視為滑動，不觸發點擊彈出 */
const TAP_MOVE_THRESHOLD_PX = 10
/** 按住超過此時間視為長按，不觸發點擊彈出 */
const TAP_MAX_DURATION_MS = 350

/** 全視窗閱讀：捲動時隱藏頂部標題與底部控制列；點擊畫面任意處彈出（需與滑動區分） */
export function useReaderChromeAutoHide() {
  const chromeVisible = ref(true)
  let touchStartX = 0
  let touchStartY = 0
  let touchStartTime = 0
  let touchMoved = false

  function resetTouchTracking() {
    touchMoved = false
    touchStartTime = 0
  }

  function onReaderScrollForChrome() {
    if (!readerFullscreenActive.value) {
      if (!chromeVisible.value) chromeVisible.value = true
      return
    }
    chromeVisible.value = false
  }

  function onReaderTouchStart(e: TouchEvent) {
    if (!readerFullscreenActive.value) return
    if (e.touches.length !== 1) {
      resetTouchTracking()
      return
    }
    const touch = e.touches[0]
    touchStartX = touch.clientX
    touchStartY = touch.clientY
    touchStartTime = Date.now()
    touchMoved = false
  }

  function onReaderTouchMove(e: TouchEvent) {
    if (!readerFullscreenActive.value || touchStartTime === 0) return
    if (e.touches.length !== 1) {
      touchMoved = true
      return
    }
    const touch = e.touches[0]
    const dx = Math.abs(touch.clientX - touchStartX)
    const dy = Math.abs(touch.clientY - touchStartY)
    if (dx > TAP_MOVE_THRESHOLD_PX || dy > TAP_MOVE_THRESHOLD_PX) {
      touchMoved = true
    }
  }

  function onReaderTouchEnd() {
    if (!readerFullscreenActive.value || touchStartTime === 0) {
      resetTouchTracking()
      return
    }
    const isTap =
      !touchMoved && Date.now() - touchStartTime <= TAP_MAX_DURATION_MS
    resetTouchTracking()
    if (isTap) chromeVisible.value = true
  }

  function onReaderTouchCancel() {
    resetTouchTracking()
  }

  watch(readerFullscreenActive, (fs) => {
    resetTouchTracking()
    chromeVisible.value = true
    if (!fs) chromeVisible.value = true
  })

  onBeforeUnmount(() => {
    resetTouchTracking()
  })

  return {
    chromeVisible,
    onReaderScrollForChrome,
    onReaderTouchStart,
    onReaderTouchMove,
    onReaderTouchEnd,
    onReaderTouchCancel,
  }
}
