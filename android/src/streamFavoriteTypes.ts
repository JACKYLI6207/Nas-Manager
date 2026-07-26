import type { RemoteStreamJob } from './videoStreamStore'

/** 串流收藏主分類（video/comic 各自獨立儲存，結構共用） */
export type FavoriteStreamCategory = {
  id: string
  name: string
  createdAt: number
}

/** 收藏名稱（播放列表），一定歸屬於某個主分類 */
export type FavoriteStreamPlaylistBase = {
  id: string
  name: string
  categoryId: string
  jobs: RemoteStreamJob[]
  starredAt: number
}

export type FavoriteStreamStateBase = {
  categories: FavoriteStreamCategory[]
  playlists: FavoriteStreamPlaylistBase[]
}

/**
 * video / comic 收藏儲存共用介面，供 `StreamFavoritesPanel.vue` 等共用 UI 呼叫，
 * 避免重複實作分類／收藏名稱／項目的增刪改邏輯。
 */
export type FavoriteStreamAdapter<
  TState extends FavoriteStreamStateBase = FavoriteStreamStateBase,
  TJob extends RemoteStreamJob = RemoteStreamJob,
> = {
  /** UI 顯示用單位（影片＝部，漫畫＝本） */
  unitLabel: string
  defaultCategoryId: string
  defaultCategoryName: string
  loadState(): TState
  addCategory(name: string): TState
  renameCategory(id: string, name: string): TState
  /** 刪除主分類：其收藏移入未分類；刪未分類本身須為空，否則拋出錯誤訊息 */
  removeCategory(id: string): TState
  addPlaylist(name: string, jobs: TJob[], categoryId?: string): TState
  /** 加入現有收藏：附加 jobs 並依 jobKey 去重 */
  addJobsToPlaylist(playlistId: string, jobs: TJob[]): TState
  renamePlaylist(id: string, name: string): TState
  removePlaylist(id: string): TState
  removePlaylistItem(playlistId: string, job: TJob): TState
  /** 移動收藏名稱到另一主分類；同名時自動加上「 (2)」等後綴 */
  movePlaylistToCategory(playlistId: string, categoryId: string): TState
  /** 移動項目：必須指定目標收藏名稱（不可只選主分類） */
  moveItemToPlaylist(sourcePlaylistId: string, targetPlaylistId: string, job: TJob): TState
  /** 同層排序：direction -1=上移、1=下移；已在邊界則不變 */
  reorderCategory(id: string, direction: -1 | 1): TState
  reorderPlaylist(id: string, direction: -1 | 1): TState
  reorderPlaylistItem(playlistId: string, job: TJob, direction: -1 | 1): TState
  /** 導入收藏：覆蓋寫入完整狀態 */
  replaceState(state: TState): TState
  jobKey(job: TJob): string
}

/** 遠端多選資料夾收藏時，每個勾選根路徑一組 */
export type FavoriteJobGroup = {
  defaultName: string
  jobs: RemoteStreamJob[]
}
