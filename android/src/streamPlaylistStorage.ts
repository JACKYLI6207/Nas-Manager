import type { RemoteStreamJob } from './videoStreamStore'

const STORAGE_KEY = 'gmFavoriteStreamPlaylists'

export type FavoriteStreamPlaylist = {
  id: string
  name: string
  jobs: RemoteStreamJob[]
  starredAt: number
}

export function jobKey(job: RemoteStreamJob): string {
  return `${job.host.trim().toLowerCase()}:${job.port}:${job.relPath}`
}

export function loadFavoriteStreamPlaylists(): FavoriteStreamPlaylist[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter((item): item is FavoriteStreamPlaylist => {
        if (typeof item !== 'object' || item === null) return false
        const p = item as FavoriteStreamPlaylist
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

export function saveFavoriteStreamPlaylists(playlists: FavoriteStreamPlaylist[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(playlists))
}

export function addFavoriteStreamPlaylist(
  name: string,
  jobs: RemoteStreamJob[],
): FavoriteStreamPlaylist[] {
  const trimmed = name.trim() || `播放列表 ${new Date().toLocaleString('zh-TW')}`
  const entry: FavoriteStreamPlaylist = {
    id: `pl_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    name: trimmed,
    jobs: jobs.map((j) => ({ ...j })),
    starredAt: Date.now(),
  }
  const list = [entry, ...loadFavoriteStreamPlaylists()]
  saveFavoriteStreamPlaylists(list)
  return list
}

export function renameFavoriteStreamPlaylist(id: string, name: string): FavoriteStreamPlaylist[] {
  const trimmed = name.trim()
  if (!trimmed) return loadFavoriteStreamPlaylists()
  const list = loadFavoriteStreamPlaylists().map((p) =>
    p.id === id ? { ...p, name: trimmed } : p,
  )
  saveFavoriteStreamPlaylists(list)
  return list
}

export function removeFavoriteStreamPlaylist(id: string): FavoriteStreamPlaylist[] {
  const list = loadFavoriteStreamPlaylists().filter((p) => p.id !== id)
  saveFavoriteStreamPlaylists(list)
  return list
}

export function removeFavoriteStreamPlaylistItem(
  playlistId: string,
  job: RemoteStreamJob,
): FavoriteStreamPlaylist[] {
  const key = jobKey(job)
  const list = loadFavoriteStreamPlaylists().map((p) => {
    if (p.id !== playlistId) return p
    return { ...p, jobs: p.jobs.filter((j) => jobKey(j) !== key) }
  })
  saveFavoriteStreamPlaylists(list)
  return list
}
