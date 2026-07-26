import {
  gridColsForRemoteBrowseLayout,
  isRemoteBrowseGridLayout,
  REMOTE_BROWSE_LAYOUT_OPTIONS,
  remoteBrowseLayoutLabel,
  type RemoteBrowseLayout,
} from './remoteBrowseLayout'

/** 串流收藏顯示方式沿用遠端瀏覽的版面定義，但各自獨立儲存（影片／漫畫分開）。 */
export type StreamFavoriteLayout = RemoteBrowseLayout
export type StreamFavoriteMedia = 'video' | 'comic'

export {
  REMOTE_BROWSE_LAYOUT_OPTIONS as STREAM_FAVORITE_LAYOUT_OPTIONS,
  gridColsForRemoteBrowseLayout as gridColsForStreamFavoriteLayout,
  isRemoteBrowseGridLayout as isStreamFavoriteGridLayout,
  remoteBrowseLayoutLabel as streamFavoriteLayoutLabel,
}

const VALID = new Set<StreamFavoriteLayout>([
  'list',
  'grid1',
  'grid2',
  'grid4',
  'grid6',
  'grid8',
  'grid10',
])

function layoutStorageKey(media: StreamFavoriteMedia): string {
  return `nas-stream-fav-layout-${media}`
}

export function loadStreamFavoriteLayout(media: StreamFavoriteMedia): StreamFavoriteLayout {
  try {
    const saved = localStorage.getItem(layoutStorageKey(media))
    if (saved && VALID.has(saved as StreamFavoriteLayout)) {
      return saved as StreamFavoriteLayout
    }
  } catch {
    /* ignore */
  }
  return 'list'
}

export function saveStreamFavoriteLayout(media: StreamFavoriteMedia, layout: StreamFavoriteLayout) {
  try {
    localStorage.setItem(layoutStorageKey(media), layout)
  } catch {
    /* ignore */
  }
}
