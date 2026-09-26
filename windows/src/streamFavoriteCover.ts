import { ref } from 'vue'
import {
  fetchRemoteComicPageImage,
  fetchRemoteComicPages,
  listRemotePcDirectory,
} from './api'
import {
  COMIC_LIST_MARKER,
  findCoverSiblingDir,
  resolveSiblingCoverUrl,
  VIDEO_LIST_MARKER,
} from './remoteBrowseCover'
import { buildRemoteStreamUrl, isComicZipFileName, isImageFile } from './remoteBrowseMedia'
import { resolveStreamJobHost } from './resolveStreamJobHost'
import type { RemoteStreamJob } from './videoStreamStore'

export type StreamFavoriteCoverKind = 'video' | 'comic'

function normPath(p: string): string {
  return p.replace(/\\/g, '/').replace(/^\/+/, '')
}

function baseName(p: string): string {
  const n = normPath(p).replace(/\/+$/, '')
  const i = n.lastIndexOf('/')
  return i >= 0 ? n.slice(i + 1) : n
}

function dirName(p: string): string {
  const n = normPath(p)
  const i = n.lastIndexOf('/')
  return i >= 0 ? n.slice(0, i) : ''
}

function byNameCollator(a: string, b: string): number {
  return a.localeCompare(b, 'zh-Hant', { numeric: true, sensitivity: 'base' })
}

/** 僅快取成功的 URL；帶 TTL，PC 換圖後無需重開 App */
const COVER_TTL_MS = 45_000
type CoverCacheEntry = { url: string; at: number }
const coverUrlCache = new Map<string, CoverCacheEntry>()
const inflight = new Map<string, Promise<string | null>>()
/** 清快取時遞增，縮圖元件 watch 後重載 */
export const streamFavoriteCoverEpoch = ref(0)

/** 限制同時解 ZIP 首頁的數量，避免一次展開大量漫畫時請求打掛 */
let zipSlotActive = 0
const zipWaiters: Array<() => void> = []
const ZIP_MAX_PARALLEL = 2

async function withZipSlot<T>(fn: () => Promise<T>): Promise<T> {
  if (zipSlotActive >= ZIP_MAX_PARALLEL) {
    await new Promise<void>((resolve) => zipWaiters.push(resolve))
  }
  zipSlotActive += 1
  try {
    return await fn()
  } finally {
    zipSlotActive -= 1
    zipWaiters.shift()?.()
  }
}

/** 以「目前可達 host」為鍵，避免區網收藏在 Tailscale 連線時仍命中舊 LAN URL */
function cacheKey(
  host: string,
  port: number,
  relPath: string,
  kind: StreamFavoriteCoverKind,
): string {
  return `${kind}:${host.trim().toLowerCase()}:${port}:${normPath(relPath)}`
}

async function findImageByIndexPairing(
  host: string,
  port: number,
  job: RemoteStreamJob,
  coverDir: string,
  matchFile: (name: string) => boolean,
): Promise<string | null> {
  try {
    const jobDir = dirName(job.relPath)
    const jobName = baseName(job.relPath)
    const [siblingResult, coverResult] = await Promise.all([
      listRemotePcDirectory(host, port, jobDir),
      listRemotePcDirectory(host, port, coverDir),
    ])
    const siblings = siblingResult.entries
      .filter((e) => !e.isDir && matchFile(e.name))
      .map((e) => e.name)
      .sort(byNameCollator)
    const covers = coverResult.entries
      .filter((e) => !e.isDir && isImageFile(e.name))
      .map((e) => e.name)
      .sort(byNameCollator)
    const idx = siblings.findIndex((n) => n === jobName)
    if (idx < 0 || idx >= covers.length) return null
    const coverRel = coverDir ? `${coverDir}/${covers[idx]}` : covers[idx]!
    return buildRemoteStreamUrl(host, port, coverRel)
  } catch {
    return null
  }
}

async function findComicZipFirstPageUrl(
  job: RemoteStreamJob,
  host: string,
  port: number,
): Promise<string | null> {
  return withZipSlot(async () => {
    try {
      const pages = await fetchRemoteComicPages(host, port, job.relPath)
      const first = pages.pages[0]
      if (!first) return null
      const bytes = await fetchRemoteComicPageImage(host, port, job.relPath, first.entry)
      const blob = new Blob([new Uint8Array(bytes)], { type: 'image/jpeg' })
      return URL.createObjectURL(blob)
    } catch {
      return null
    }
  })
}

/**
 * 解析串流收藏項目的封面 URL：
 * - 影片：`.../影片清單/...` → 同層 `封面清單` 同檔名圖片
 * - 漫畫：封面清單（同名→索引配對）→ ZIP 第一頁
 * - 先 `resolveStreamJobHost`（區網／Tailscale），與播放一致；遠端管理縮圖用當前連線 host，收藏不可死釘 job.host
 * 失敗不永久快取，可重試。
 */
function bustStreamUrl(url: string): string {
  if (url.startsWith('blob:')) return url
  const sep = url.includes('?') ? '&' : '?'
  return `${url}${sep}_=${Date.now()}`
}

export async function resolveStreamFavoriteCoverUrl(
  job: RemoteStreamJob,
  kind: StreamFavoriteCoverKind,
): Promise<string | null> {
  let host = job.host
  let port = job.port
  try {
    const resolved = await resolveStreamJobHost(job)
    host = resolved.host
    port = resolved.port
  } catch {
    /* 用原 host 繼續 */
  }

  const key = cacheKey(host, port, job.relPath, kind)
  const cached = coverUrlCache.get(key)
  if (cached && Date.now() - cached.at <= COVER_TTL_MS) return cached.url
  if (cached) {
    if (cached.url.startsWith('blob:')) {
      try {
        URL.revokeObjectURL(cached.url)
      } catch {
        /* ignore */
      }
    }
    coverUrlCache.delete(key)
  }
  const existing = inflight.get(key)
  if (existing) return existing

  const task = (async () => {
    const marker = kind === 'video' ? VIDEO_LIST_MARKER : COMIC_LIST_MARKER
    const coverDir = findCoverSiblingDir(job.relPath, marker)
    let url: string | null = null
    if (coverDir) {
      url = await resolveSiblingCoverUrl(host, port, job.relPath, marker)
      if (!url && kind === 'comic') {
        url = await findImageByIndexPairing(host, port, job, coverDir, isComicZipFileName)
      }
    }
    // 漫畫：無封面圖或路徑不含「漫畫清單」時，一律試 ZIP 首頁
    if (!url && kind === 'comic' && isComicZipFileName(baseName(job.relPath))) {
      url = await findComicZipFirstPageUrl(job, host, port)
    }
    if (url) {
      const finalUrl = bustStreamUrl(url)
      coverUrlCache.set(key, { url: finalUrl, at: Date.now() })
      return finalUrl
    }
    return url
  })()
  inflight.set(key, task)
  try {
    return await task
  } finally {
    inflight.delete(key)
  }
}

export function clearStreamFavoriteCoverCache() {
  for (const entry of coverUrlCache.values()) {
    if (entry.url.startsWith('blob:')) {
      try {
        URL.revokeObjectURL(entry.url)
      } catch {
        /* ignore */
      }
    }
  }
  coverUrlCache.clear()
  streamFavoriteCoverEpoch.value += 1
}

let lastCoverRefreshAt = 0

/** 進入串流收藏時呼叫：距上次超過 TTL 才清快取重抓，避免切頁閃爍 */
export function maybeRefreshStreamFavoriteCovers() {
  const now = Date.now()
  if (now - lastCoverRefreshAt < COVER_TTL_MS) return
  lastCoverRefreshAt = now
  clearStreamFavoriteCoverCache()
}

/** 清除單一項目失敗／過期快取，供縮圖重試 */
export function invalidateStreamFavoriteCover(job: RemoteStreamJob, kind: StreamFavoriteCoverKind) {
  const host = job.host.trim().toLowerCase()
  const path = normPath(job.relPath)
  for (const key of [...coverUrlCache.keys()]) {
    if (key.startsWith(`${kind}:`) && key.endsWith(`:${job.port}:${path}`)) {
      const prev = coverUrlCache.get(key)
      if (prev?.url.startsWith('blob:')) {
        try {
          URL.revokeObjectURL(prev.url)
        } catch {
          /* ignore */
        }
      }
      coverUrlCache.delete(key)
    }
  }
  // 亦清原 host 鍵（相容）
  coverUrlCache.delete(cacheKey(host, job.port, job.relPath, kind))
}
