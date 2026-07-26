import { listRemotePcFiles } from './api'
import { buildRemoteStreamUrl, isImageFile } from './remoteBrowseMedia'

/** 與串流收藏相同的資料夾架構標記 */
export const VIDEO_LIST_MARKER = '影片清單'
export const COMIC_LIST_MARKER = '漫畫清單'
export const COVER_LIST_DIR = '封面清單'

function normPath(p: string): string {
  return p.replace(/\\/g, '/').replace(/^\/+/, '')
}

function baseName(p: string): string {
  const n = normPath(p).replace(/\/+$/, '')
  const i = n.lastIndexOf('/')
  return i >= 0 ? n.slice(i + 1) : n
}

function stemOf(name: string): string {
  const i = name.lastIndexOf('.')
  return i > 0 ? name.slice(0, i) : name
}

/**
 * 封面比對候選 stem（小寫，由長到短）：
 * - 精確：ABCD-1234-1
 * - 產品碼+分段：ABCD-1234-1 → ABCD-1234（封面常只有主檔名）
 * - 一般名稱分段：Title-1 → Title
 * 不會把 ABCD-1234 誤剝成 ABCD-12。
 */
export function coverStemCandidates(stem: string): string[] {
  const k = stem.trim().toLowerCase()
  if (!k) return []
  const out: string[] = []
  const seen = new Set<string>()
  const push = (s: string) => {
    const t = s.trim()
    if (!t || seen.has(t)) return
    seen.add(t)
    out.push(t)
  }
  push(k)
  // 1) ABCD-1234-1 / ABCD-1234_2（主檔名已含 -數字）
  const productPart = k.match(/^(.+-\d+)[-_](\d{1,2})$/)
  if (productPart?.[1]) {
    push(productPart[1])
    return out
  }
  // 2) Title-1 / Movie_02（base 不以數字結尾，避免誤剝產品碼）
  const namedPart = k.match(/^(.*[^0-9])[-_](\d{1,2})$/)
  if (namedPart?.[1]) {
    push(namedPart[1].replace(/[-_]+$/, ''))
  }
  return out
}

/** 找出 relPath 內 marker 資料夾同層的「封面清單」路徑；找不到回傳 null。 */
export function findCoverSiblingDir(relPath: string, marker: string): string | null {
  const norm = normPath(relPath)
  const markerSlash = `${marker}/`
  if (norm.startsWith(markerSlash)) {
    return COVER_LIST_DIR
  }
  const idx = norm.indexOf(`/${markerSlash}`)
  if (idx < 0) return null
  const parent = norm.slice(0, idx)
  return `${parent}/${COVER_LIST_DIR}`
}

/** stem(小寫) → 封面相對路徑 */
type CoverIndex = Map<string, string>

const COVER_INDEX_TTL_MS = 60_000
const coverIndexCache = new Map<string, { at: number; index: CoverIndex }>()
const inflightIndex = new Map<string, Promise<CoverIndex>>()

function indexCacheKey(host: string, port: number, coverDir: string): string {
  return `${host.trim().toLowerCase()}:${port}:${normPath(coverDir)}`
}

async function loadCoverIndex(host: string, port: number, coverDir: string): Promise<CoverIndex> {
  const key = indexCacheKey(host, port, coverDir)
  const hit = coverIndexCache.get(key)
  if (hit && Date.now() - hit.at <= COVER_INDEX_TTL_MS) return hit.index

  const existing = inflightIndex.get(key)
  if (existing) return existing

  const task = (async () => {
    const index: CoverIndex = new Map()
    try {
      const files = await listRemotePcFiles(host, port, coverDir)
      for (const f of files) {
        const name = baseName(f.relativePath)
        if (!isImageFile(name)) continue
        const stem = stemOf(name).toLowerCase()
        if (!stem || index.has(stem)) continue
        index.set(stem, f.relativePath.replace(/\\/g, '/'))
      }
      // 成功列出（含空目錄）才快取；連線失敗不寫入，避免整批縮圖被空索引卡住
      coverIndexCache.set(key, { at: Date.now(), index })
    } catch {
      /* 無封面清單或連線失敗 → 不快取，下次可重試 */
    }
    return index
  })()

  inflightIndex.set(key, task)
  try {
    return await task
  } finally {
    inflightIndex.delete(key)
  }
}

/**
 * 在「影片清單／漫畫清單」+ 同層「封面清單」架構下，
 * 依媒體檔 stem 找同名封面圖 URL（與串流收藏相同規則）。
 * 分段影片（ABCD-1234-1…）會回退比對主檔名封面（ABCD-1234）。
 */
export async function resolveSiblingCoverUrl(
  host: string,
  port: number,
  mediaRelPath: string,
  marker: string,
  cacheBust?: number | string,
): Promise<string | null> {
  const coverDir = findCoverSiblingDir(mediaRelPath, marker)
  if (!coverDir) return null
  const stem = stemOf(baseName(mediaRelPath))
  if (!stem) return null
  const index = await loadCoverIndex(host, port, coverDir)
  for (const candidate of coverStemCandidates(stem)) {
    const coverRel = index.get(candidate)
    if (coverRel) {
      return buildRemoteStreamUrl(host, port, coverRel, cacheBust)
    }
  }
  return null
}

/** 目錄重整時清掉封面索引，避免沿用舊清單 */
export function clearRemoteBrowseCoverIndexCache(host?: string, port?: number) {
  const prefix =
    host != null && port != null ? `${host.trim().toLowerCase()}:${port}:` : null
  for (const key of [...coverIndexCache.keys()]) {
    if (prefix && !key.startsWith(prefix)) continue
    coverIndexCache.delete(key)
  }
}
