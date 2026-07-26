import { ref } from 'vue'
import { normalizePcName } from './remotePcName'
import { defaultStreamFavoriteName } from './streamFavoriteName'
import type { FavoriteStreamAdapter } from './streamFavoriteTypes'
import type { RemoteStreamJob } from './videoStreamStore'

const STORAGE_KEY = 'gmFavoriteStreamPlaylists'

export const UNCATEGORIZED_CATEGORY_ID = 'cat_default'
export const UNCATEGORIZED_CATEGORY_NAME = '未分類'

/** 收藏列表變更時遞增，供分頁徽章等 reactive 讀取 */
export const videoFavoritePlaylistsRevision = ref(0)

export type FavoriteStreamCategory = {
  id: string
  name: string
  createdAt: number
}

export type FavoriteStreamPlaylist = {
  id: string
  name: string
  categoryId: string
  jobs: RemoteStreamJob[]
  starredAt: number
}

export type FavoriteStreamState = {
  categories: FavoriteStreamCategory[]
  playlists: FavoriteStreamPlaylist[]
}

/** 進度／收藏去重鍵：優先用 pcName（跨 IP），否則退回 host。 */
export function jobKey(job: RemoteStreamJob): string {
  const id = job.pcName?.trim()
    ? normalizePcName(job.pcName)
    : job.host.trim().toLowerCase()
  return `${id}:${job.port}:${job.relPath}`
}

function randomId(prefix: string): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`
}

function makeDefaultCategory(): FavoriteStreamCategory {
  return { id: UNCATEGORIZED_CATEGORY_ID, name: UNCATEGORIZED_CATEGORY_NAME, createdAt: 0 }
}

function isValidCategory(c: unknown): c is FavoriteStreamCategory {
  if (typeof c !== 'object' || c === null) return false
  const p = c as FavoriteStreamCategory
  return typeof p.id === 'string' && typeof p.name === 'string'
}

function isValidPlaylist(p: unknown): p is FavoriteStreamPlaylist {
  if (typeof p !== 'object' || p === null) return false
  const v = p as FavoriteStreamPlaylist
  return (
    typeof v.id === 'string' &&
    typeof v.name === 'string' &&
    Array.isArray(v.jobs) &&
    typeof v.starredAt === 'number'
  )
}

/** 相容舊格式（純陣列）與新格式（{categories, playlists}），並補齊缺失的未分類。 */
function normalizeState(raw: unknown): { state: FavoriteStreamState; migrated: boolean } {
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

export function loadFavoriteStreamState(): FavoriteStreamState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) {
      const state: FavoriteStreamState = { categories: [makeDefaultCategory()], playlists: [] }
      saveFavoriteStreamState(state)
      return state
    }
    const parsed: unknown = JSON.parse(raw)
    const { state, migrated } = normalizeState(parsed)
    // 保留陣列順序（供上移／下移）；不再依 starredAt 重排
    if (migrated) saveFavoriteStreamState(state)
    return state
  } catch {
    return { categories: [makeDefaultCategory()], playlists: [] }
  }
}

export function saveFavoriteStreamState(state: FavoriteStreamState) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
  videoFavoritePlaylistsRevision.value += 1
}

/** 導入覆蓋：正規化後整包寫入 */
export function replaceFavoriteStreamState(raw: unknown): FavoriteStreamState {
  const { state } = normalizeState(raw)
  saveFavoriteStreamState(state)
  return state
}

/** 相容舊呼叫端：回傳攤平的收藏名稱陣列 */
export function loadFavoriteStreamPlaylists(): FavoriteStreamPlaylist[] {
  return loadFavoriteStreamState().playlists
}

function ensureCategoryExists(
  categories: FavoriteStreamCategory[],
  id: string,
): FavoriteStreamCategory[] {
  if (categories.some((c) => c.id === id)) return categories
  if (id === UNCATEGORIZED_CATEGORY_ID) return [makeDefaultCategory(), ...categories]
  return categories
}

export function addFavoriteCategory(name: string): FavoriteStreamState {
  const trimmed = name.trim()
  const state = loadFavoriteStreamState()
  if (!trimmed) return state
  const cat: FavoriteStreamCategory = { id: randomId('cat'), name: trimmed, createdAt: Date.now() }
  const next: FavoriteStreamState = { ...state, categories: [...state.categories, cat] }
  saveFavoriteStreamState(next)
  return next
}

export function renameFavoriteCategory(id: string, name: string): FavoriteStreamState {
  const trimmed = name.trim()
  const state = loadFavoriteStreamState()
  if (!trimmed) return state
  const categories = state.categories.map((c) => (c.id === id ? { ...c, name: trimmed } : c))
  const next: FavoriteStreamState = { ...state, categories }
  saveFavoriteStreamState(next)
  return next
}

/** 刪除主分類：其收藏移入未分類；刪未分類本身須為空，否則拋出錯誤。 */
export function removeFavoriteCategory(id: string): FavoriteStreamState {
  const state = loadFavoriteStreamState()
  if (!state.categories.some((c) => c.id === id)) return state

  if (id === UNCATEGORIZED_CATEGORY_ID) {
    const hasAny = state.playlists.some((p) => p.categoryId === id)
    if (hasAny) {
      throw new Error('未分類內尚有收藏，請先移出或刪除後再刪除本分類')
    }
    const next: FavoriteStreamState = {
      ...state,
      categories: state.categories.filter((c) => c.id !== id),
    }
    saveFavoriteStreamState(next)
    return next
  }

  let categories = ensureCategoryExists(state.categories, UNCATEGORIZED_CATEGORY_ID)
  categories = categories.filter((c) => c.id !== id)
  const playlists = state.playlists.map((p) =>
    p.categoryId === id ? { ...p, categoryId: UNCATEGORIZED_CATEGORY_ID } : p,
  )
  const next: FavoriteStreamState = { categories, playlists }
  saveFavoriteStreamState(next)
  return next
}

export function addFavoriteStreamPlaylist(
  name: string,
  jobs: RemoteStreamJob[],
  categoryId?: string,
): FavoriteStreamState {
  const state = loadFavoriteStreamState()
  const trimmed =
    name.trim() ||
    defaultStreamFavoriteName(jobs, `播放列表 ${new Date().toLocaleString('zh-TW')}`)
  let categories = state.categories
  let targetCatId = categoryId
  if (!targetCatId || !categories.some((c) => c.id === targetCatId)) {
    categories = ensureCategoryExists(categories, UNCATEGORIZED_CATEGORY_ID)
    targetCatId = UNCATEGORIZED_CATEGORY_ID
  }
  const entry: FavoriteStreamPlaylist = {
    id: randomId('pl'),
    name: trimmed,
    categoryId: targetCatId,
    jobs: jobs.map((j) => ({ ...j })),
    starredAt: Date.now(),
  }
  const next: FavoriteStreamState = { categories, playlists: [entry, ...state.playlists] }
  saveFavoriteStreamState(next)
  return next
}

/** 加入現有收藏：附加 jobs 並依 jobKey 去重 */
export function addJobsToFavoriteStreamPlaylist(
  playlistId: string,
  jobs: RemoteStreamJob[],
): FavoriteStreamState {
  const state = loadFavoriteStreamState()
  const playlists = state.playlists.map((p) => {
    if (p.id !== playlistId) return p
    const existing = new Set(p.jobs.map(jobKey))
    const appended = jobs.filter((j) => !existing.has(jobKey(j))).map((j) => ({ ...j }))
    return { ...p, jobs: [...p.jobs, ...appended] }
  })
  const next: FavoriteStreamState = { ...state, playlists }
  saveFavoriteStreamState(next)
  return next
}

export function renameFavoriteStreamPlaylist(id: string, name: string): FavoriteStreamState {
  const trimmed = name.trim()
  const state = loadFavoriteStreamState()
  if (!trimmed) return state
  const playlists = state.playlists.map((p) => (p.id === id ? { ...p, name: trimmed } : p))
  const next: FavoriteStreamState = { ...state, playlists }
  saveFavoriteStreamState(next)
  return next
}

export function removeFavoriteStreamPlaylist(id: string): FavoriteStreamState {
  const state = loadFavoriteStreamState()
  const next: FavoriteStreamState = {
    ...state,
    playlists: state.playlists.filter((p) => p.id !== id),
  }
  saveFavoriteStreamState(next)
  return next
}

export function removeFavoriteStreamPlaylistItem(
  playlistId: string,
  job: RemoteStreamJob,
): FavoriteStreamState {
  const key = jobKey(job)
  const state = loadFavoriteStreamState()
  const playlists = state.playlists.map((p) => {
    if (p.id !== playlistId) return p
    return { ...p, jobs: p.jobs.filter((j) => jobKey(j) !== key) }
  })
  const next: FavoriteStreamState = { ...state, playlists }
  saveFavoriteStreamState(next)
  return next
}

/** 移動收藏名稱到另一主分類；同名時自動加上「 (2)」等後綴。 */
export function moveFavoriteStreamPlaylistToCategory(
  playlistId: string,
  categoryId: string,
): FavoriteStreamState {
  const state = loadFavoriteStreamState()
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
  const next: FavoriteStreamState = { ...state, playlists }
  saveFavoriteStreamState(next)
  return next
}

/** 移動項目：必須指定目標收藏名稱；依 jobKey 去重。 */
export function moveFavoriteStreamItemToPlaylist(
  sourcePlaylistId: string,
  targetPlaylistId: string,
  job: RemoteStreamJob,
): FavoriteStreamState {
  const state = loadFavoriteStreamState()
  if (sourcePlaylistId === targetPlaylistId) return state
  const key = jobKey(job)
  let movedJob: RemoteStreamJob | null = null
  const afterRemove = state.playlists.map((p) => {
    if (p.id !== sourcePlaylistId) return p
    const found = p.jobs.find((j) => jobKey(j) === key)
    if (found) movedJob = found
    return { ...p, jobs: p.jobs.filter((j) => jobKey(j) !== key) }
  })
  if (!movedJob) return state
  const finalMovedJob: RemoteStreamJob = movedJob
  const playlists = afterRemove.map((p) => {
    if (p.id !== targetPlaylistId) return p
    if (p.jobs.some((j) => jobKey(j) === key)) return p
    return { ...p, jobs: [...p.jobs, { ...finalMovedJob }] }
  })
  const next: FavoriteStreamState = { ...state, playlists }
  saveFavoriteStreamState(next)
  return next
}

/** 主分類列表內上移／下移 */
export function reorderFavoriteCategory(id: string, direction: -1 | 1): FavoriteStreamState {
  const state = loadFavoriteStreamState()
  const idx = state.categories.findIndex((c) => c.id === id)
  if (idx < 0) return state
  const swap = idx + direction
  if (swap < 0 || swap >= state.categories.length) return state
  const categories = [...state.categories]
  ;[categories[idx], categories[swap]] = [categories[swap]!, categories[idx]!]
  const next: FavoriteStreamState = { ...state, categories }
  saveFavoriteStreamState(next)
  return next
}

/** 同一主分類下，收藏名稱上移／下移 */
export function reorderFavoritePlaylist(id: string, direction: -1 | 1): FavoriteStreamState {
  const state = loadFavoriteStreamState()
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
  const next: FavoriteStreamState = { ...state, playlists }
  saveFavoriteStreamState(next)
  return next
}

/** 同一收藏內，項目上移／下移 */
export function reorderFavoritePlaylistItem(
  playlistId: string,
  job: RemoteStreamJob,
  direction: -1 | 1,
): FavoriteStreamState {
  const key = jobKey(job)
  const state = loadFavoriteStreamState()
  const playlists = state.playlists.map((p) => {
    if (p.id !== playlistId) return p
    const idx = p.jobs.findIndex((j) => jobKey(j) === key)
    const swap = idx + direction
    if (idx < 0 || swap < 0 || swap >= p.jobs.length) return p
    const jobs = [...p.jobs]
    ;[jobs[idx], jobs[swap]] = [jobs[swap]!, jobs[idx]!]
    return { ...p, jobs }
  })
  const next: FavoriteStreamState = { ...state, playlists }
  saveFavoriteStreamState(next)
  return next
}

export const videoFavoriteAdapter: FavoriteStreamAdapter<FavoriteStreamState, RemoteStreamJob> = {
  unitLabel: '部',
  defaultCategoryId: UNCATEGORIZED_CATEGORY_ID,
  defaultCategoryName: UNCATEGORIZED_CATEGORY_NAME,
  loadState: loadFavoriteStreamState,
  addCategory: addFavoriteCategory,
  renameCategory: renameFavoriteCategory,
  removeCategory: removeFavoriteCategory,
  addPlaylist: addFavoriteStreamPlaylist,
  addJobsToPlaylist: addJobsToFavoriteStreamPlaylist,
  renamePlaylist: renameFavoriteStreamPlaylist,
  removePlaylist: removeFavoriteStreamPlaylist,
  removePlaylistItem: removeFavoriteStreamPlaylistItem,
  movePlaylistToCategory: moveFavoriteStreamPlaylistToCategory,
  moveItemToPlaylist: moveFavoriteStreamItemToPlaylist,
  reorderCategory: reorderFavoriteCategory,
  reorderPlaylist: reorderFavoritePlaylist,
  reorderPlaylistItem: reorderFavoritePlaylistItem,
  replaceState: replaceFavoriteStreamState,
  jobKey,
}
