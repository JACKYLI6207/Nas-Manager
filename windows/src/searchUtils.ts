export type SearchSortOrder =
  | 'createDateAsc'
  | 'createDateDesc'
  | 'titleAsc'
  | 'titleDesc'
  | 'comicIdAsc'
  | 'comicIdDesc'

export const DEFAULT_SEARCH_SORT_ORDER: SearchSortOrder = 'comicIdDesc'

/** 官網即時列表分頁軸（與 HTML 列表順序一致，約 ID 降序） */
export const WEBSITE_BROWSE_SORT_ORDER: SearchSortOrder = 'comicIdDesc'

export const LIVE_BROWSE_CUSTOM_SORT_HINT =
  '目前排序僅重排本頁，與官網分頁不一致；對照官網請選「ID編號降序」'

export function isIdBasedSortOrder(order: SearchSortOrder): boolean {
  return order === 'comicIdDesc' || order === 'comicIdAsc'
}

const SORT_KEY = 'gm-android-searchSortOrder'
const PAGE_SIZE_KEY = 'gm-android-searchPageSize'
const LAYOUT_KEY = 'gm-android-searchLayout'

const FAV_SORT_KEY = 'gm-android-favoritesSortOrder'
const FAV_PAGE_SIZE_KEY = 'gm-android-favoritesPageSize'
const FAV_LAYOUT_KEY = 'gm-android-favoritesLayout'

/** 與 PC comicBrowseLayout 對齊；手機端 list 為橫向緊湊列表 */
export type GridLayout = 'list' | 'grid2' | 'grid4' | 'grid6' | 'grid8' | 'grid10'

export const PAGE_SIZE_OPTIONS = [20, 40, 60, 80, 100] as const

export const SEARCH_SORT_OPTIONS: { label: string; key: SearchSortOrder }[] = [
  { label: 'ID編號降序', key: 'comicIdDesc' },
  { label: 'ID編號升序', key: 'comicIdAsc' },
  { label: '創建日期降序', key: 'createDateDesc' },
  { label: '創建日期升序', key: 'createDateAsc' },
]

export const LAYOUT_OPTIONS: { key: GridLayout; label: string }[] = [
  { key: 'grid2', label: '每排 2 個' },
  { key: 'grid4', label: '每排 4 個' },
  { key: 'grid6', label: '每排 6 個' },
  { key: 'grid8', label: '每排 8 個' },
  { key: 'grid10', label: '每排 10 個' },
  { key: 'list', label: '列表顯示' },
]

export function gridColsForLayout(layout: GridLayout): number {
  switch (layout) {
    case 'grid2':
      return 2
    case 'grid6':
      return 6
    case 'grid8':
      return 8
    case 'grid10':
      return 10
    case 'grid4':
    default:
      return 4
  }
}

function parseComicCreatedAt(additionalInfo: string): number {
  const match = additionalInfo.match(/創建於(\d{4}-\d{2}-\d{2})(?:\s+(\d{2}):(\d{2}):(\d{2}))?/)
  if (match === null) return 0
  const hour = match[2] ?? '00'
  const minute = match[3] ?? '00'
  const second = match[4] ?? '00'
  const timestamp = Date.parse(`${match[1]}T${hour}:${minute}:${second}`)
  return Number.isNaN(timestamp) ? 0 : timestamp
}

export function sortSearchComics<T extends { id: number; title: string; additionalInfo: string }>(
  comics: T[],
  order: SearchSortOrder,
): T[] {
  const sorted = [...comics]
  sorted.sort((a, b) => {
    switch (order) {
      case 'createDateAsc':
        return parseComicCreatedAt(a.additionalInfo) - parseComicCreatedAt(b.additionalInfo)
      case 'createDateDesc':
        return parseComicCreatedAt(b.additionalInfo) - parseComicCreatedAt(a.additionalInfo)
      case 'titleAsc':
        return a.title.localeCompare(b.title, 'zh-Hant', { numeric: true })
      case 'titleDesc':
        return b.title.localeCompare(a.title, 'zh-Hant', { numeric: true })
      case 'comicIdAsc':
        return a.id - b.id
      case 'comicIdDesc':
        return b.id - a.id
    }
  })
  return sorted
}

export function loadSavedSortOrder(): SearchSortOrder {
  const saved = localStorage.getItem(SORT_KEY)
  return SEARCH_SORT_OPTIONS.some((o) => o.key === saved) ? (saved as SearchSortOrder) : DEFAULT_SEARCH_SORT_ORDER
}

export function saveSortOrder(order: SearchSortOrder) {
  localStorage.setItem(SORT_KEY, order)
}

export function loadSavedPageSize(): number {
  const saved = localStorage.getItem(PAGE_SIZE_KEY)
  const n = saved !== null ? parseInt(saved, 10) : 20
  return PAGE_SIZE_OPTIONS.includes(n as (typeof PAGE_SIZE_OPTIONS)[number]) ? n : 20
}

export function savePageSize(size: number) {
  localStorage.setItem(PAGE_SIZE_KEY, String(size))
}

const VALID_LAYOUTS = new Set<GridLayout>(['list', 'grid2', 'grid4', 'grid6', 'grid8', 'grid10'])

export function loadSavedLayout(): GridLayout {
  const saved = localStorage.getItem(LAYOUT_KEY)
  if (saved && VALID_LAYOUTS.has(saved as GridLayout)) {
    return saved as GridLayout
  }
  return 'grid4'
}

export function saveLayout(layout: GridLayout) {
  localStorage.setItem(LAYOUT_KEY, layout)
}

export function loadSavedFavoritesSortOrder(): SearchSortOrder {
  const saved = localStorage.getItem(FAV_SORT_KEY)
  return SEARCH_SORT_OPTIONS.some((o) => o.key === saved) ? (saved as SearchSortOrder) : DEFAULT_SEARCH_SORT_ORDER
}

export function saveFavoritesSortOrder(order: SearchSortOrder) {
  localStorage.setItem(FAV_SORT_KEY, order)
}

export function loadSavedFavoritesPageSize(): number {
  const saved = localStorage.getItem(FAV_PAGE_SIZE_KEY)
  const n = saved !== null ? parseInt(saved, 10) : 20
  return PAGE_SIZE_OPTIONS.includes(n as (typeof PAGE_SIZE_OPTIONS)[number]) ? n : 20
}

export function saveFavoritesPageSize(size: number) {
  localStorage.setItem(FAV_PAGE_SIZE_KEY, String(size))
}

export function loadSavedFavoritesLayout(): GridLayout {
  const saved = localStorage.getItem(FAV_LAYOUT_KEY)
  if (saved && VALID_LAYOUTS.has(saved as GridLayout)) {
    return saved as GridLayout
  }
  return 'grid4'
}

export function saveFavoritesLayout(layout: GridLayout) {
  localStorage.setItem(FAV_LAYOUT_KEY, layout)
}
