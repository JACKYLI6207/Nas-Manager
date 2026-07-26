import type { StreamFavoriteMedia } from './streamFavoriteLayout'
import type { FavoriteStreamStateBase } from './streamFavoriteTypes'

export const STREAM_FAVORITE_EXPORT_KIND = 'nas-stream-favorites' as const

export type StreamFavoriteExportPayload = {
  kind: typeof STREAM_FAVORITE_EXPORT_KIND
  media: StreamFavoriteMedia
  version: 1
  exportedAt: number
  state: FavoriteStreamStateBase
}

export function buildStreamFavoriteExportJson(
  media: StreamFavoriteMedia,
  state: FavoriteStreamStateBase,
): string {
  const payload: StreamFavoriteExportPayload = {
    kind: STREAM_FAVORITE_EXPORT_KIND,
    media,
    version: 1,
    exportedAt: Date.now(),
    state,
  }
  return `${JSON.stringify(payload, null, 2)}\n`
}

export function defaultStreamFavoriteExportFileName(media: StreamFavoriteMedia): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  const stamp = `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}_${pad(d.getHours())}${pad(d.getMinutes())}`
  const prefix = media === 'video' ? 'nas-video-favorites' : 'nas-comic-favorites'
  return `${prefix}-${stamp}.json`
}

/** 解析導出檔；回傳可交給 replaceState 的 state 物件 */
export function parseStreamFavoriteImportText(
  text: string,
  expectedMedia: StreamFavoriteMedia,
): FavoriteStreamStateBase {
  let parsed: unknown
  try {
    parsed = JSON.parse(text)
  } catch {
    throw new Error('無法解析 JSON 檔案')
  }
  if (!parsed || typeof parsed !== 'object') {
    throw new Error('不是有效的收藏匯出檔')
  }
  const obj = parsed as Record<string, unknown>
  if (obj.kind === STREAM_FAVORITE_EXPORT_KIND) {
    if (obj.media !== expectedMedia) {
      const label = obj.media === 'video' ? '影片' : obj.media === 'comic' ? '漫畫' : '其他'
      const here = expectedMedia === 'video' ? '影片播放' : '漫畫閱讀'
      throw new Error(`此檔為${label}串流收藏，無法導入到「${here}」`)
    }
    if (!obj.state || typeof obj.state !== 'object') {
      throw new Error('匯出檔缺少 state')
    }
    return obj.state as FavoriteStreamStateBase
  }
  // 相容直接匯出的 { categories, playlists }
  if (Array.isArray(obj.categories) && Array.isArray(obj.playlists)) {
    return obj as unknown as FavoriteStreamStateBase
  }
  throw new Error('不是有效的收藏匯出檔')
}
