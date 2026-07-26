import { ref } from 'vue'
import type { RemoteComicStreamJob } from './comicStreamStore'
import { comicJobKey } from './comicStreamStore'
import { defaultStreamFavoriteName } from './streamFavoriteName'
import type { FavoriteStreamAdapter } from './streamFavoriteTypes'

const STORAGE_KEY = 'gmFavoriteComicStreamPlaylists'

export const UNCATEGORIZED_CATEGORY_ID = 'ccat_default'
export const UNCATEGORIZED_CATEGORY_NAME = '未分類'

/** 收藏列表變更時遞增，供子分頁徽章等 reactive 讀取 */
export const comicFavoritePlaylistsRevision = ref(0)

export type FavoriteComicStreamCategory = {
  id: string
  name: string
  createdAt: number
}

export type FavoriteComicStreamPlaylist = {
  id: string
  name: string
  categoryId: string
  jobs: RemoteComicStreamJob[]
  starredAt: number
}

export type FavoriteComicStreamState = {
  categories: FavoriteComicStreamCategory[]
  playlists: FavoriteComicStreamPlaylist[]
}

function randomId(prefix: string): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

function makeDefaultCategory(): FavoriteComicStreamCategory {
  return { id: UNCATEGORIZED_CATEGORY_ID, name: UNCATEGORIZED_CATEGORY_NAME, createdAt: 0 }
}

function isValidCategory(c: unknown): c is FavoriteComicStreamCategory {
  if (typeof c !== 'object' || c === null) return false
  const p = c as FavoriteComicStreamCategory
  return typeof p.id === 'string' && typeof p.name === 'string'
}

function isValidPlaylist(p: unknown): p is FavoriteComicStreamPlaylist {
  if (typeof p !== 'object' || p === null) return false
  const v = p as FavoriteComicStreamPlaylist
  return (
    typeof v.id === 'string' &&
    typeof v.name === 'string' &&
    Array.isArray(v.jobs) &&
    typeof v.starredAt === 'number'
  )
}

/** 相容舊格式（純陣列）與新格式（{categories, playlists}），並補齊缺失的未分類。 */
function normalizeState(raw: unknown): { state: FavoriteComicStreamState; migrated: boolean } {
  if (Array.isArray(raw)) {
    const def = makeDefaultCategory()
    const playlists = raw
      .filter(isValidPlaylist)
      .map((p) => ({ ...p, categoryId: UNCATEGORIZED_CATEGORY_ID }))
    return { state: { categories: [def], playlists }, migrated: true }
  }
  if (raw && typeof raw === 'object') {
    const obj = raw as { categories?: unknown; playlists?: unknown }
    let categories = Array.isArray(obj.categories) ? obj.categories.filter(isValidCategory) : []
    let migrated = !Array.isArray(obj.categories)
    if (!categories.some((c) => c.id === UNCATEGORIZED_CATEGORY_ID)) {
      categories = [makeDefaultCategory(), ...categories]
      migrated = true
    }
    const knownIds = new Set(categories.map((c) => c.id))
    const rawPlaylists = Array.isArray(obj.playlists) ? obj.playlists.filter(isValidPlaylist) : []
    if (!Array.isArray(obj.playlists)) migrated = true
    const playlists = rawPlaylists.map((p) => {
      const categoryId = (p as { categoryId?: unknown }).categoryId
      if (typeof categoryId !== 'string' || !knownIds.has(categoryId)) {
        migrated = true
        return { ...p, categoryId: UNCATEGORIZED_CATEGORY_ID }
      }
      return p
    })
    return { state: { categories, playlists }, migrated }
  }
  return { state: { categories: [makeDefaultCategory()], playlists: [] }, migrated: true }
}

export function loadFavoriteComicStreamState(): FavoriteComicStreamState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) {
      const state: FavoriteComicStreamState = { categories: [makeDefaultCategory()], playlists: [] }
      saveFavoriteComicStreamState(state)
      return state
    }
    const parsed: unknown = JSON.parse(raw)
    const { state, migrated } = normalizeState(parsed)
    // 保留陣列順序（供上移／下移）；不再依 starredAt 重排
    if (migrated) saveFavoriteComicStreamState(state)
    return state
  } catch {
    return { categories: [makeDefaultCategory()], playlists: [] }
  }
}

export function saveFavoriteComicStreamState(state: FavoriteComicStreamState) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  comicFavoritePlaylistsRevision.value += 1
}

/** 導入覆蓋：正規化後整包寫入 */
export function replaceFavoriteComicStreamState(raw: unknown): FavoriteComicStreamState {
  const { state } = normalizeState(raw)
  saveFavoriteComicStreamState(state)
  return state
}

/** 相容舊呼叫端：回傳攤平的收藏名稱陣列 */
export function loadFavoriteComicStreamPlaylists(): FavoriteComicStreamPlaylist[] {
  return loadFavoriteComicStreamState().playlists
}

function ensureCategoryExists(
  categories: FavoriteComicStreamCategory[],
  id: string,
): FavoriteComicStreamCategory[] {
  if (categories.some((c) => c.id === id)) return categories
  if (id === UNCATEGORIZED_CATEGORY_ID) return [makeDefaultCategory(), ...categories]
  return categories
}

export function addFavoriteComicCategory(name: string): FavoriteComicStreamState {
  const trimmed = name.trim()
  const state = loadFavoriteComicStreamState()
  if (!trimmed) return state
  const cat: FavoriteComicStreamCategory = {
    id: randomId('ccat'),
    name: trimmed,
    createdAt: Date.now(),
  }
  const next: FavoriteComicStreamState = { ...state, categories: [...state.categories, cat] }
  saveFavoriteComicStreamState(next)
  return next
}

export function renameFavoriteComicCategory(id: string, name: string): FavoriteComicStreamState {
  const trimmed = name.trim()
  const state = loadFavoriteComicStreamState()
  if (!trimmed) return state
  const categories = state.categories.map((c) => (c.id === id ? { ...c, name: trimmed } : c))
  const next: FavoriteComicStreamState = { ...state, categories }
  saveFavoriteComicStreamState(next)
  return next
}

/** 刪除主分類：其收藏移入未分類；刪未分類本身須為空，否則拋出錯誤。 */
export function removeFavoriteComicCategory(id: string): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  if (!state.categories.some((c) => c.id === id)) return state

  if (id === UNCATEGORIZED_CATEGORY_ID) {
    const hasAny = state.playlists.some((p) => p.categoryId === id)
    if (hasAny) {
      throw new Error('未分類內尚有收藏，請先移出或刪除後再刪除本分類')
    }
    const next: FavoriteComicStreamState = {
      ...state,
      categories: state.categories.filter((c) => c.id !== id),
    }
    saveFavoriteComicStreamState(next)
    return next
  }

  let categories = ensureCategoryExists(state.categories, UNCATEGORIZED_CATEGORY_ID)
  categories = categories.filter((c) => c.id !== id)
  const playlists = state.playlists.map((p) =>
    p.categoryId === id ? { ...p, categoryId: UNCATEGORIZED_CATEGORY_ID } : p,
  )
  const next: FavoriteComicStreamState = { categories, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

export function addFavoriteComicStreamPlaylist(
  name: string,
  jobs: RemoteComicStreamJob[],
  categoryId?: string,
): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  const trimmed =
    name.trim() ||
    defaultStreamFavoriteName(jobs, `漫畫列表 ${new Date().toLocaleString('zh-TW')}`)
  let categories = state.categories
  let targetCatId = categoryId
  if (!targetCatId || !categories.some((c) => c.id === targetCatId)) {
    categories = ensureCategoryExists(categories, UNCATEGORIZED_CATEGORY_ID)
    targetCatId = UNCATEGORIZED_CATEGORY_ID
  }
  const entry: FavoriteComicStreamPlaylist = {
    id: randomId('cpl'),
    name: trimmed,
    categoryId: targetCatId,
    jobs: jobs.map((j) => ({ ...j })),
    starredAt: Date.now(),
  }
  const next: FavoriteComicStreamState = { categories, playlists: [entry, ...state.playlists] }
  saveFavoriteComicStreamState(next)
  return next
}

/** 加入現有收藏：附加 jobs 並依 jobKey 去重 */
export function addJobsToFavoriteComicStreamPlaylist(
  playlistId: string,
  jobs: RemoteComicStreamJob[],
): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  const playlists = state.playlists.map((p) => {
    if (p.id !== playlistId) return p
    const existing = new Set(p.jobs.map(comicJobKey))
    const appended = jobs.filter((j) => !existing.has(comicJobKey(j))).map((j) => ({ ...j }))
    return { ...p, jobs: [...p.jobs, ...appended] }
  })
  const next: FavoriteComicStreamState = { ...state, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

export function renameFavoriteComicStreamPlaylist(
  id: string,
  name: string,
): FavoriteComicStreamState {
  const trimmed = name.trim()
  const state = loadFavoriteComicStreamState()
  if (!trimmed) return state
  const playlists = state.playlists.map((p) => (p.id === id ? { ...p, name: trimmed } : p))
  const next: FavoriteComicStreamState = { ...state, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

export function removeFavoriteComicStreamPlaylist(id: string): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  const next: FavoriteComicStreamState = {
    ...state,
    playlists: state.playlists.filter((p) => p.id !== id),
  }
  saveFavoriteComicStreamState(next)
  return next
}

export function removeFavoriteComicStreamPlaylistItem(
  playlistId: string,
  job: RemoteComicStreamJob,
): FavoriteComicStreamState {
  const key = comicJobKey(job)
  const state = loadFavoriteComicStreamState()
  const playlists = state.playlists.map((p) => {
    if (p.id !== playlistId) return p
    return { ...p, jobs: p.jobs.filter((j) => comicJobKey(j) !== key) }
  })
  const next: FavoriteComicStreamState = { ...state, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

/** 移動收藏名稱到另一主分類；同名時自動加上「 (2)」等後綴。 */
export function moveFavoriteComicStreamPlaylistToCategory(
  playlistId: string,
  categoryId: string,
): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  const target = state.playlists.find((p) => p.id === playlistId)
  if (!target || target.categoryId === categoryId) return state
  const siblingNames = new Set(
    state.playlists.filter((p) => p.categoryId === categoryId && p.id !== playlistId).map((p) => p.name),
  )
  let finalName = target.name
  if (siblingNames.has(finalName)) {
    let n = 2
    do {
      finalName = `${target.name} (${n})`
      n += 1
    } while (siblingNames.has(finalName))
  }
  const playlists = state.playlists.map((p) =>
    p.id === playlistId ? { ...p, categoryId, name: finalName } : p,
  )
  const next: FavoriteComicStreamState = { ...state, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

/** 移動項目：必須指定目標收藏名稱；依 jobKey 去重。 */
export function moveFavoriteComicStreamItemToPlaylist(
  sourcePlaylistId: string,
  targetPlaylistId: string,
  job: RemoteComicStreamJob,
): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  if (sourcePlaylistId === targetPlaylistId) return state
  const key = comicJobKey(job)
  let movedJob: RemoteComicStreamJob | null = null
  const afterRemove = state.playlists.map((p) => {
    if (p.id !== sourcePlaylistId) return p
    const found = p.jobs.find((j) => comicJobKey(j) === key)
    if (found) movedJob = found
    return { ...p, jobs: p.jobs.filter((j) => comicJobKey(j) !== key) }
  })
  if (!movedJob) return state
  const finalMovedJob: RemoteComicStreamJob = movedJob
  const playlists = afterRemove.map((p) => {
    if (p.id !== targetPlaylistId) return p
    if (p.jobs.some((j) => comicJobKey(j) === key)) return p
    return { ...p, jobs: [...p.jobs, { ...finalMovedJob }] }
  })
  const next: FavoriteComicStreamState = { ...state, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

/** 主分類列表內上移／下移 */
export function reorderFavoriteComicCategory(id: string, direction: -1 | 1): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  const idx = state.categories.findIndex((c) => c.id === id)
  if (idx < 0) return state
  const swap = idx + direction
  if (swap < 0 || swap >= state.categories.length) return state
  const categories = [...state.categories]
  ;[categories[idx], categories[swap]] = [categories[swap]!, categories[idx]!]
  const next: FavoriteComicStreamState = { ...state, categories }
  saveFavoriteComicStreamState(next)
  return next
}

/** 同一主分類下，收藏名稱上移／下移 */
export function reorderFavoriteComicPlaylist(id: string, direction: -1 | 1): FavoriteComicStreamState {
  const state = loadFavoriteComicStreamState()
  const pl = state.playlists.find((p) => p.id === id)
  if (!pl) return state
  const siblingIdxs = state.playlists
    .map((p, i) => (p.categoryId === pl.categoryId ? i : -1))
    .filter((i) => i >= 0)
  const pos = siblingIdxs.findIndex((i) => state.playlists[i]!.id === id)
  const targetPos = pos + direction
  if (pos < 0 || targetPos < 0 || targetPos >= siblingIdxs.length) return state
  const from = siblingIdxs[pos]!
  const to = siblingIdxs[targetPos]!
  const playlists = [...state.playlists]
  ;[playlists[from], playlists[to]] = [playlists[to]!, playlists[from]!]
  const next: FavoriteComicStreamState = { ...state, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

/** 同一收藏內，項目上移／下移 */
export function reorderFavoriteComicPlaylistItem(
  playlistId: string,
  job: RemoteComicStreamJob,
  direction: -1 | 1,
): FavoriteComicStreamState {
  const key = comicJobKey(job)
  const state = loadFavoriteComicStreamState()
  const playlists = state.playlists.map((p) => {
    if (p.id !== playlistId) return p
    const idx = p.jobs.findIndex((j) => comicJobKey(j) === key)
    const swap = idx + direction
    if (idx < 0 || swap < 0 || swap >= p.jobs.length) return p
    const jobs = [...p.jobs]
    ;[jobs[idx], jobs[swap]] = [jobs[swap]!, jobs[idx]!]
    return { ...p, jobs }
  })
  const next: FavoriteComicStreamState = { ...state, playlists }
  saveFavoriteComicStreamState(next)
  return next
}

export const comicFavoriteAdapter: FavoriteStreamAdapter<
  FavoriteComicStreamState,
  RemoteComicStreamJob
> = {
  unitLabel: '本',
  defaultCategoryId: UNCATEGORIZED_CATEGORY_ID,
  defaultCategoryName: UNCATEGORIZED_CATEGORY_NAME,
  loadState: loadFavoriteComicStreamState,
  addCategory: addFavoriteComicCategory,
  renameCategory: renameFavoriteComicCategory,
  removeCategory: removeFavoriteComicCategory,
  addPlaylist: addFavoriteComicStreamPlaylist,
  addJobsToPlaylist: addJobsToFavoriteComicStreamPlaylist,
  renamePlaylist: renameFavoriteComicStreamPlaylist,
  removePlaylist: removeFavoriteComicStreamPlaylist,
  removePlaylistItem: removeFavoriteComicStreamPlaylistItem,
  movePlaylistToCategory: moveFavoriteComicStreamPlaylistToCategory,
  moveItemToPlaylist: moveFavoriteComicStreamItemToPlaylist,
  reorderCategory: reorderFavoriteComicCategory,
  reorderPlaylist: reorderFavoriteComicPlaylist,
  reorderPlaylistItem: reorderFavoriteComicPlaylistItem,
  replaceState: replaceFavoriteComicStreamState,
  jobKey: comicJobKey,
}

export { comicJobKey as jobKeyForComicPlaylist }
