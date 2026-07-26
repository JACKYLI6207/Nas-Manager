import { isVideoFileName } from './remoteBrowseMedia'

/** 從串流 jobs 推斷共用父資料夾名稱，供收藏預設命名。 */
export function defaultStreamFavoriteName(
  jobs: { relPath: string; title: string }[],
  fallback: string,
): string {
  const folder = commonTargetFolderName(jobs)
  return folder || fallback
}

function commonTargetFolderName(jobs: { relPath: string; title: string }[]): string {
  if (jobs.length === 0) return ''

  if (jobs.length === 1) {
    return folderNameForSingleJob(jobs[0]!)
  }

  const parents = jobs.map((j) => parentDir(j.relPath))
  const firstParent = parents[0] ?? ''
  if (firstParent && parents.every((p) => p === firstParent)) {
    return baseName(firstParent)
  }

  // 多筆父層不完全相同時，取最長共用路徑（例：電影/A、電影/B →「電影」）
  const lcp = longestCommonPathPrefix(parents.filter(Boolean))
  if (lcp) return baseName(lcp)

  return ''
}

/** 單筆：影片用所在資料夾名；漫畫 ZIP／資料夾用自身名稱。 */
function folderNameForSingleJob(job: { relPath: string; title: string }): string {
  const leaf = baseName(job.relPath) || job.title
  if (isVideoFileName(leaf)) {
    const parent = parentDir(job.relPath)
    if (parent) return baseName(parent)
    return stripMediaExt(leaf) || job.title
  }
  return stripArchiveExt(leaf) || job.title
}

function parentDir(relPath: string): string {
  const p = relPath.replace(/\\/g, '/').replace(/\/+$/, '')
  const i = p.lastIndexOf('/')
  return i >= 0 ? p.slice(0, i) : ''
}

function baseName(path: string): string {
  const p = path.replace(/\\/g, '/').replace(/\/+$/, '')
  const segs = p.split('/').filter(Boolean)
  if (segs.length === 0) return ''
  try {
    return decodeURIComponent(segs[segs.length - 1]!)
  } catch {
    return segs[segs.length - 1]!
  }
}

function longestCommonPathPrefix(paths: string[]): string {
  if (paths.length === 0) return ''
  const parts = paths.map((p) => p.replace(/\\/g, '/').split('/').filter(Boolean))
  const minLen = Math.min(...parts.map((p) => p.length))
  if (minLen <= 0) return ''
  const out: string[] = []
  for (let i = 0; i < minLen; i++) {
    const seg = parts[0]![i]!
    if (!parts.every((p) => p[i] === seg)) break
    out.push(seg)
  }
  return out.join('/')
}

function stripArchiveExt(name: string): string {
  return name.replace(/\.(zip|cbz|rar|cbr|7z)$/i, '')
}

function stripMediaExt(name: string): string {
  return name.replace(/\.(mp4|mkv|avi|mov|wmv|flv|webm|m4v|ts|m2ts|rmvb|rm)$/i, '')
}
