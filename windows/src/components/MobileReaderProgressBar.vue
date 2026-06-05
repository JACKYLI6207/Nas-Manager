<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import {
  beginReaderSeek,
  endReaderSeek,
  isReaderProgressSeekDisabled,
  readerImageLoadProgress,
  readerScrollProgress,
  seekReaderByRatio,
  setReaderSeekPreview,
} from '../readerProgressBridge'

defineProps<{
  overlay?: boolean
}>()

const trackRef = ref<HTMLElement | null>(null)
const dragging = ref(false)
const dragRatio = ref(0)
/** WebView 常同時觸發 pointer + touch，避免鬆手 commit 兩次 */
let seekCommitted = false
let activePointerId: number | null = null

const seekDisabled = computed(() => isReaderProgressSeekDisabled())

const displayRatio = computed(() => {
  const r = dragging.value ? dragRatio.value : readerScrollProgress.ratio
  return Math.min(1, Math.max(0, r))
})

const labelText = computed(() => readerScrollProgress.pageLabel || '閱讀進度')

const seekHint = computed(() => {
  if (seekDisabled.value) return '還原閱讀位置中…'
  return ''
})

function ratioFromClientX(clientX: number): number {
  const track = trackRef.value
  if (!track) return 0
  const rect = track.getBoundingClientRect()
  if (rect.width <= 0) return 0
  return Math.min(1, Math.max(0, (clientX - rect.left) / rect.width))
}

/** 拖曳中只更新滑桿與頁碼預覽，不捲動、不觸發載入 */
function previewDrag(ratio: number) {
  dragRatio.value = ratio
  setReaderSeekPreview(ratio)
}

function startDrag(clientX: number) {
  if (seekDisabled.value) return
  if (dragging.value) return
  seekCommitted = false
  dragging.value = true
  beginReaderSeek()
  previewDrag(ratioFromClientX(clientX))
}

function moveDrag(clientX: number) {
  if (!dragging.value) return
  previewDrag(ratioFromClientX(clientX))
}

function endDrag() {
  if (!dragging.value || seekCommitted) return
  seekCommitted = true
  const finalRatio = dragRatio.value
  dragging.value = false
  activePointerId = null
  seekReaderByRatio(finalRatio)
}

function onPointerDown(ev: PointerEvent) {
  if (seekDisabled.value || ev.button !== 0) return
  const track = trackRef.value
  if (!track) return
  ev.preventDefault()
  activePointerId = ev.pointerId
  startDrag(ev.clientX)
  try {
    track.setPointerCapture(ev.pointerId)
  } catch {
    /* 部分 WebView 不支援 */
  }
}

function onPointerMove(ev: PointerEvent) {
  if (!dragging.value || activePointerId !== ev.pointerId) return
  ev.preventDefault()
  moveDrag(ev.clientX)
}

function onPointerUp(ev: PointerEvent) {
  if (activePointerId !== null && ev.pointerId !== activePointerId) return
  endDrag()
}

function onPointerCancel(ev: PointerEvent) {
  if (activePointerId !== null && ev.pointerId !== activePointerId) return
  endDrag()
}

onBeforeUnmount(() => {
  if (dragging.value) {
    dragging.value = false
    endReaderSeek()
  }
})
</script>

<template>
  <div
    class="reader-progress-dock"
    :class="{ 'reader-progress-dock--overlay': overlay }"
  >
    <div class="reader-progress-label-row">
      <span
        v-if="readerImageLoadProgress.total > 0"
        class="reader-load-label"
        title="背景預載進度（鬆手後才跳轉並載入）"
      >
        {{ readerImageLoadProgress.loaded }}/{{ readerImageLoadProgress.total }}
      </span>
      <span v-else class="reader-load-label reader-load-label--spacer" aria-hidden="true" />
      <span class="reader-progress-label">{{ labelText }}</span>
      <span v-if="seekHint" class="reader-load-label reader-seek-lock-hint" :title="seekHint">
        🔒
      </span>
      <span v-else class="reader-load-label reader-load-label--spacer" aria-hidden="true" />
    </div>
    <div
      ref="trackRef"
      class="reader-progress-track"
      :class="{ 'reader-progress-track--disabled': seekDisabled }"
      :aria-disabled="seekDisabled"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerCancel"
    >
      <div class="reader-progress-fill" :style="{ width: `${displayRatio * 100}%` }" />
      <div class="reader-progress-thumb" :style="{ left: `${displayRatio * 100}%` }" />
    </div>
  </div>
</template>

<style scoped>
.reader-progress-dock {
  flex-shrink: 0;
  padding: 4px 12px 6px;
  border-top: 1px solid #2a4a6e;
  background: #1a1a1a;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.reader-progress-dock--overlay {
  position: fixed;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 100010;
  padding-bottom: max(env(safe-area-inset-bottom, 0px), 6px);
  box-shadow: 0 -4px 16px rgba(0, 0, 0, 0.45);
}

.reader-progress-label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  min-height: 14px;
}

.reader-load-label {
  flex: 0 0 auto;
  min-width: 3.2rem;
  font-size: 10px;
  color: #6eb5ff;
  line-height: 1.2;
  pointer-events: none;
  user-select: none;
  font-variant-numeric: tabular-nums;
}

.reader-load-label--spacer {
  visibility: hidden;
}

.reader-seek-lock-hint {
  min-width: auto;
  opacity: 0.85;
}

.reader-progress-label {
  flex: 1 1 auto;
  font-size: 11px;
  color: #9ab;
  text-align: center;
  line-height: 1.2;
  pointer-events: none;
  user-select: none;
}

.reader-progress-track {
  position: relative;
  height: 28px;
  padding: 10px 0;
  box-sizing: border-box;
  touch-action: none;
  cursor: pointer;
  user-select: none;
}

.reader-progress-track--disabled {
  cursor: not-allowed;
  opacity: 0.55;
  touch-action: none;
}

.reader-progress-track--disabled .reader-progress-thumb {
  border-color: #666;
  box-shadow: none;
}

.reader-progress-track::before {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  height: 6px;
  margin-top: -3px;
  border-radius: 3px;
  background: #333;
}

.reader-progress-fill {
  position: absolute;
  left: 0;
  top: 50%;
  height: 6px;
  margin-top: -3px;
  border-radius: 3px 0 0 3px;
  background: linear-gradient(90deg, #3d6ef5, #5b8fff);
  pointer-events: none;
  max-width: 100%;
}

.reader-progress-thumb {
  position: absolute;
  top: 50%;
  width: 16px;
  height: 16px;
  margin-top: -8px;
  transform: translateX(-50%);
  border-radius: 50%;
  background: #fff;
  border: 2px solid #3d6ef5;
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.35);
  pointer-events: none;
}
</style>
