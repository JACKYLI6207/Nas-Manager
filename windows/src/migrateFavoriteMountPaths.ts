import { comicFavoriteAdapter } from './comicStreamPlaylistStorage'
import {
  migrateJobsRelPaths,
  type ShareMountInfo,
} from './streamFavoritePathMigrate'
import { videoFavoriteAdapter } from './streamPlaylistStorage'

/** 連上 PC 根目錄後：把舊磁碟代號 label 收藏改寫成 Volume GUID label（不影響仍可解析的舊路徑） */
export function migrateStreamFavoritesMountPaths(mounts: ShareMountInfo[]): boolean {
  if (!mounts.length) return false
  let any = false

  const videoState = videoFavoriteAdapter.loadState()
  let videoChanged = false
  const videoPlaylists = videoState.playlists.map((pl) => {
    const { jobs, changed } = migrateJobsRelPaths(pl.jobs, mounts)
    if (changed) videoChanged = true
    return changed ? { ...pl, jobs } : pl
  })
  if (videoChanged) {
    videoFavoriteAdapter.replaceState({ ...videoState, playlists: videoPlaylists })
    any = true
  }

  const comicState = comicFavoriteAdapter.loadState()
  let comicChanged = false
  const comicPlaylists = comicState.playlists.map((pl) => {
    const { jobs, changed } = migrateJobsRelPaths(pl.jobs, mounts)
    if (changed) comicChanged = true
    return changed ? { ...pl, jobs } : pl
  })
  if (comicChanged) {
    comicFavoriteAdapter.replaceState({ ...comicState, playlists: comicPlaylists })
    any = true
  }

  return any
}
