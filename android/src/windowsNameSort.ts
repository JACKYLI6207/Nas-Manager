/**
 * 近似 Windows 10 檔案總管「名稱」排序（StrCmpLogicalW）：
 * 數字按數值比較（2 < 10）、大小寫不敏感、依系統語系字元順序。
 */
const windowsNameCollator = new Intl.Collator(['zh-Hant', 'zh-TW', 'en-US'], {
  numeric: true,
  sensitivity: 'base',
  ignorePunctuation: false,
})

export function compareWindowsFileName(a: string, b: string): number {
  return windowsNameCollator.compare(a ?? '', b ?? '')
}

/** 只對 siblings 中「被選取」的項目就地重新排序，未選取位置不變 */
export function sortSelectedSlotsByName<T>(
  siblings: T[],
  isSelected: (item: T) => boolean,
  nameOf: (item: T) => string,
  ascending: boolean,
): T[] {
  const indices: number[] = []
  const picked: T[] = []
  for (let i = 0; i < siblings.length; i++) {
    const item = siblings[i]!
    if (!isSelected(item)) continue
    indices.push(i)
    picked.push(item)
  }
  if (picked.length < 2) return siblings

  picked.sort((x, y) => {
    const c = compareWindowsFileName(nameOf(x), nameOf(y))
    return ascending ? c : -c
  })

  const next = siblings.slice()
  for (let i = 0; i < indices.length; i++) {
    next[indices[i]!] = picked[i]!
  }
  return next
}
