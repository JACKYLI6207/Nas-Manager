import { ref } from 'vue'

/** 在線／本地閱讀器共用，全視窗時隱藏主介面 chrome */
export const readerFullscreenActive = ref(false)

export function useReaderFullscreen() {
  function toggleFullscreen() {
    readerFullscreenActive.value = !readerFullscreenActive.value
  }

  function exitFullscreen() {
    readerFullscreenActive.value = false
  }

  return {
    isFullscreen: readerFullscreenActive,
    toggleFullscreen,
    exitFullscreen,
  }
}
