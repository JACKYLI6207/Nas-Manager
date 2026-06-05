import type { RemoteComicStreamJob } from './comicStreamStore'
import { comicJobKey } from './comicStreamStore'
import { ref } from 'vue'

const STORAGE_KEY = 'gmFavoriteComicStreamPlaylists'

/** 收藏列表變更時遞增，供子分頁徽章等 reactive 讀取 */
export const comicFavoritePlaylistsRevision = ref(0)

export type FavoriteComicStreamPlaylist = {
  id: string
  name: string
  jobs: RemoteComicStreamJob[]
  starredAt: number
}

export function loadFavoriteComicStreamPlaylists(): FavoriteComicStreamPlaylist[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter((item): item is FavoriteComicStreamPlaylist => {
        if (typeof item !== 'object' || item === null) return false
        const p = item as FavoriteComicStreamPlaylist
        return (
          typeof p.id === 'string' &&
          typeof p.name === 'string' &&
          Array.isArray(p.jobs) &&
          typeof p.starredAt === 'number'
        )
      })
      .sort((a, b) => b.starredAt - a.starredAt)
  } catch {
    return []
  }
}

export function saveFavoriteComicStreamPlaylists(playlists: FavoriteComicStreamPlaylist[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(playlists))
  comicFavoritePlaylistsRevision.value += 1
}

export function addFavoriteComicStreamPlaylist(
  name: string,
  jobs: RemoteComicStreamJob[],
): FavoriteComicStreamPlaylist[] {
  const trimmed = name.trim() || `漫畫列表 ${new Date().toLocaleString('zh-TW')}`
  const entry: FavoriteComicStreamPlaylist = {
    id: `cpl_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    name: trimmed,
    jobs: jobs.map((j) => ({ ...j })),
    starredAt: Date.now(),
  }
  const list = [entry, ...loadFavoriteComicStreamPlaylists()]
  saveFavoriteComicStreamPlaylists(list)
  return list
}

export function renameFavoriteComicStreamPlaylist(
  id: string,
  name: string,
): FavoriteComicStreamPlaylist[] {
  const trimmed = name.trim()
  if (!trimmed) return loadFavoriteComicStreamPlaylists()
  const list = loadFavoriteComicStreamPlaylists().map((p) =>
    p.id === id ? { ...p, name: trimmed } : p,
  )
  saveFavoriteComicStreamPlaylists(list)
  return list
}

export function removeFavoriteComicStreamPlaylist(id: string): FavoriteComicStreamPlaylist[] {
  const list = loadFavoriteComicStreamPlaylists().filter((p) => p.id !== id)
  saveFavoriteComicStreamPlaylists(list)
  return list
}

export function removeFavoriteComicStreamPlaylistItem(
  playlistId: string,
  job: RemoteComicStreamJob,
): FavoriteComicStreamPlaylist[] {
  const key = comicJobKey(job)
  const list = loadFavoriteComicStreamPlaylists().map((p) => {
    if (p.id !== playlistId) return p
    return { ...p, jobs: p.jobs.filter((j) => comicJobKey(j) !== key) }
  })
  saveFavoriteComicStreamPlaylists(list)
  return list
}

export { comicJobKey as jobKeyForComicPlaylist }
