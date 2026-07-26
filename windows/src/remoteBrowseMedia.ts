const IMAGE_EXTENSIONS = new Set([
  'jpg',
  'jpeg',
  'png',
  'gif',
  'webp',
  'bmp',
  'avif',
  'jfif',
])

const VIDEO_EXTENSIONS = new Set([
  'mp4',
  'mkv',
  'webm',
  'avi',
  'mov',
  'm4v',
  'ts',
  'm2ts',
  'flv',
  'wmv',
  'rmvb',
  'rm',
])

function fileExt(name: string): string {
  const idx = name.lastIndexOf('.')
  if (idx <= 0) return ''
  return name.slice(idx + 1).toLowerCase()
}

export function isImageFile(name: string): boolean {
  return IMAGE_EXTENSIONS.has(fileExt(name))
}

export function isVideoFileName(name: string): boolean {
  return VIDEO_EXTENSIONS.has(fileExt(name))
}

export function isComicZipFileName(name: string): boolean {
  const lower = name.toLowerCase()
  return lower.endsWith('.zip') || lower.endsWith('.cbz')
}

export function entryFallbackIcon(name: string, isDir: boolean): string {
  if (isDir) return '📁'
  if (isVideoFileName(name)) return '🎬'
  if (isComicZipFileName(name)) return '📚'
  if (isImageFile(name)) return '🖼'
  return '📄'
}

/** 與 Rust `URL_SAFE_NO_PAD` 對齊的 stream URL；可附 cacheBust 迫使瀏覽器重抓 */
export function buildRemoteStreamUrl(
  host: string,
  port: number,
  relPath: string,
  cacheBust?: number | string,
): string {
  const bytes = new TextEncoder().encode(relPath)
  let binary = ''
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  const b64 = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  const base = `http://${host}:${port}/api/v1/stream?path_b64=${b64}`
  if (cacheBust == null || cacheBust === '') return base
  return `${base}&_=${encodeURIComponent(String(cacheBust))}`
}

type ThumbCacheEntry = { url: string; at: number; bust: number }

/** 成功縮圖快取；同路徑重新整理才清，避免每次進目錄全量重抓 */
const THUMB_TTL_MS = 10 * 60_000
const thumbCache = new Map<string, ThumbCacheEntry>()

export function getCachedThumb(key: string): string | undefined {
  const hit = thumbCache.get(key)
  if (!hit) return undefined
  if (Date.now() - hit.at > THUMB_TTL_MS) {
    if (hit.url.startsWith('blob:')) {
      try {
        URL.revokeObjectURL(hit.url)
      } catch {
        /* ignore */
      }
    }
    thumbCache.delete(key)
    return undefined
  }
  return hit.url
}

export function setCachedThumb(key: string, url: string, bust = Date.now()) {
  const prev = thumbCache.get(key)
  if (prev?.url.startsWith('blob:') && prev.url !== url) {
    try {
      URL.revokeObjectURL(prev.url)
    } catch {
      /* ignore */
    }
  }
  thumbCache.set(key, { url, at: Date.now(), bust })
}

export function removeCachedThumb(key: string) {
  const prev = thumbCache.get(key)
  if (!prev) return
  if (prev.url.startsWith('blob:')) {
    try {
      URL.revokeObjectURL(prev.url)
    } catch {
      /* ignore */
    }
  }
  thumbCache.delete(key)
}

/** 同路徑重新整理／手動刷新時清掉該主機縮圖 */
export function clearThumbCacheForHost(host?: string, port?: number) {
  const prefix =
    host != null && port != null ? `${host.trim().toLowerCase()}:${port}:` : null
  for (const [key, entry] of [...thumbCache.entries()]) {
    if (prefix && !key.toLowerCase().startsWith(prefix)) continue
    if (entry.url.startsWith('blob:')) {
      try {
        URL.revokeObjectURL(entry.url)
      } catch {
        /* ignore */
      }
    }
    thumbCache.delete(key)
  }
}

export function clearAllThumbCache() {
  clearThumbCacheForHost()
}

export function nextThumbCacheBust(): number {
  return Date.now()
}

/** 限制同時解析／載入縮圖，避免 WebView／PC HTTP 連線被打滿造成隨機缺圖 */
const MAX_CONCURRENT_THUMB_WORK = 6
let activeThumbWork = 0
const thumbWorkQueue: Array<() => void> = []

export function scheduleThumbWork<T>(fn: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const run = () => {
      activeThumbWork += 1
      fn()
        .then(resolve, reject)
        .finally(() => {
          activeThumbWork -= 1
          const next = thumbWorkQueue.shift()
          if (next) next()
        })
    }
    if (activeThumbWork < MAX_CONCURRENT_THUMB_WORK) run()
    else thumbWorkQueue.push(run)
  })
}
