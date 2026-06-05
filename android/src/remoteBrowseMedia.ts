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

/** 與 Rust `URL_SAFE_NO_PAD` 對齊的 stream URL */
export function buildRemoteStreamUrl(host: string, port: number, relPath: string): string {
  const bytes = new TextEncoder().encode(relPath)
  let binary = ''
  for (const b of bytes) {
    binary += String.fromCharCode(b)
  }
  const b64 = btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
  return `http://${host}:${port}/api/v1/stream?path_b64=${b64}`
}

const thumbCache = new Map<string, string>()

export function getCachedThumb(key: string): string | undefined {
  return thumbCache.get(key)
}

export function setCachedThumb(key: string, url: string) {
  thumbCache.set(key, url)
}
