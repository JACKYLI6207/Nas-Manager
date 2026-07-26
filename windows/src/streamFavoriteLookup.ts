import { comicFavoriteAdapter, comicFavoritePlaylistsRevision } from './comicStreamPlaylistStorage'
import { videoFavoriteAdapter, videoFavoritePlaylistsRevision } from './streamPlaylistStorage'
import { jobKey } from './streamPlaylistStorage'
import { comicJobKey, type RemoteComicStreamJob } from './comicStreamStore'
import type { RemoteStreamJob } from './videoStreamStore'

/** 供 computed 依賴：收藏變更時重算星標 */
export function favoriteLookupRevision(): number {
  return videoFavoritePlaylistsRevision.value + comicFavoritePlaylistsRevision.value
}

function buildVideoJob(
  host: string,
  port: number,
  relPath: string,
  pcName?: string | null,
): RemoteStreamJob {
  return {
    host,
    port,
    relPath,
    title: relPath,
    pcName: pcName?.trim() || undefined,
  }
}

function buildComicJob(
  host: string,
  port: number,
  relPath: string,
  pcName?: string | null,
): RemoteComicStreamJob {
  return {
    host,
    port,
    relPath,
    title: relPath,
    pcName: pcName?.trim() || undefined,
  }
}

/** 路徑是否已在影片或漫畫串流收藏中 */
export function isRemotePathFavorited(
  host: string,
  port: number,
  relPath: string,
  pcName?: string | null,
): boolean {
  const path = relPath.replace(/\\/g, '/').replace(/^\/+/, '')
  if (!path) return false
  const vKey = jobKey(buildVideoJob(host, port, path, pcName))
  for (const p of videoFavoriteAdapter.loadState().playlists) {
    if (p.jobs.some((j) => jobKey(j) === vKey)) return true
  }
  const cKey = comicJobKey(buildComicJob(host, port, path, pcName))
  for (const p of comicFavoriteAdapter.loadState().playlists) {
    if (p.jobs.some((j) => comicJobKey(j) === cKey)) return true
  }
  return false
}
