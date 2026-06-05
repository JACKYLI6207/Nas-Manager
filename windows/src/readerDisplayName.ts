/** Android 快取 ZIP 檔名（gm-snap-uuid-…）不可當顯示標題 */
export function isGmSnapCacheTitle(title: string): boolean {
  return /^gm-snap-[0-9a-f-]{36}-/i.test(title.trim())
}

/** 解碼 content URI 最後一段或 SAF 路徑，取出可讀資料夾名（含中文） */
export function decodeFolderDisplayLabel(uriOrPath: string): string {
  const raw = uriOrPath.trim()
  if (!raw) return '已選資料夾'

  let segment = raw
  if (raw.includes('/')) {
    segment = raw.split('/').pop() ?? raw
  } else if (raw.includes('\\')) {
    segment = raw.split('\\').pop() ?? raw
  }

  try {
    segment = decodeURIComponent(segment.replace(/\+/g, ' '))
  } catch {
    /* 保留原字串 */
  }

  if (segment.includes(':')) {
    const tail = segment.split(':').pop()?.replace(/^\/+/, '').replace(/\/+$/, '') ?? segment
    if (tail.includes('/')) {
      const last = tail.split('/').filter(Boolean).pop()
      if (last) return last
    }
    if (tail && !/^primary$/i.test(tail) && !/^MuMuShared$/i.test(tail)) {
      return tail
    }
  }

  return segment || '已選資料夾'
}

export function pickLocalReaderTitle(
  readerTitle: string,
  folderSources: { path: string; label: string }[],
  currentSourcePath: string,
  currentSourceIndex: number,
): string {
  if (currentSourceIndex >= 0) {
    const fromIndex = folderSources[currentSourceIndex]?.label?.trim()
    if (fromIndex) return fromIndex
  }
  if (currentSourcePath) {
    const fromPath = folderSources.find((s) => s.path === currentSourcePath)?.label?.trim()
    if (fromPath) return fromPath
  }
  const trimmed = readerTitle.trim()
  if (trimmed && !isGmSnapCacheTitle(trimmed)) {
    return decodeFolderDisplayLabel(trimmed)
  }
  return trimmed || '漫畫閱讀'
}
