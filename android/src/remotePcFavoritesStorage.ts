const STORAGE_KEY = 'gmRemotePcFavorites'

export type RemotePcFavorite = {
  name: string
  host: string
  port: number
  starredAt: number
}

function favoriteKey(host: string, port: number): string {
  return `${host.trim().toLowerCase()}:${port}`
}

export function loadRemotePcFavorites(): RemotePcFavorite[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw === null || raw === '') return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter(
        (item): item is RemotePcFavorite =>
          typeof item === 'object' &&
          item !== null &&
          typeof (item as RemotePcFavorite).name === 'string' &&
          typeof (item as RemotePcFavorite).host === 'string' &&
          typeof (item as RemotePcFavorite).port === 'number' &&
          typeof (item as RemotePcFavorite).starredAt === 'number',
      )
      .sort((a, b) => b.starredAt - a.starredAt)
  } catch {
    return []
  }
}

export function saveRemotePcFavorites(favorites: RemotePcFavorite[]) {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(favorites))
}

export function findRemotePcFavorite(
  host: string,
  port: number,
  favorites?: RemotePcFavorite[],
): RemotePcFavorite | undefined {
  const key = favoriteKey(host, port)
  return (favorites ?? loadRemotePcFavorites()).find((f) => favoriteKey(f.host, f.port) === key)
}

export function isRemotePcFavorite(host: string, port: number, favorites: RemotePcFavorite[]): boolean {
  return findRemotePcFavorite(host, port, favorites) !== undefined
}

/** 若已收藏則回傳自訂名稱，否則 fallback（如掃描到的電腦名） */
export function getRemotePcFavoriteDisplayName(
  host: string,
  port: number,
  fallback: string,
): string {
  const fav = findRemotePcFavorite(host, port)
  if (fav && fav.name.trim()) return fav.name.trim()
  return fallback.trim() || `PC (${host})`
}

export function addRemotePcFavorite(name: string, host: string, port: number): RemotePcFavorite[] {
  const favorites = loadRemotePcFavorites()
  const key = favoriteKey(host, port)
  const trimmedHost = host.trim()
  const trimmedName = name.trim() || `PC (${trimmedHost})`
  const index = favorites.findIndex((f) => favoriteKey(f.host, f.port) === key)
  const entry: RemotePcFavorite = {
    name: trimmedName,
    host: trimmedHost,
    port,
    starredAt: Date.now(),
  }
  if (index >= 0) {
    favorites[index] = { ...entry, starredAt: favorites[index].starredAt }
  } else {
    favorites.unshift(entry)
  }
  saveRemotePcFavorites(favorites)
  return favorites
}

export function updateRemotePcFavoriteName(
  host: string,
  port: number,
  name: string,
): RemotePcFavorite[] {
  const favorites = loadRemotePcFavorites()
  const key = favoriteKey(host, port)
  const index = favorites.findIndex((f) => favoriteKey(f.host, f.port) === key)
  if (index < 0) return favorites
  const trimmed = name.trim()
  if (!trimmed) return favorites
  favorites[index] = { ...favorites[index], name: trimmed }
  saveRemotePcFavorites(favorites)
  return favorites
}

export function toggleRemotePcFavorite(
  name: string,
  host: string,
  port: number,
): RemotePcFavorite[] {
  const favorites = loadRemotePcFavorites()
  const key = favoriteKey(host, port)
  const index = favorites.findIndex((f) => favoriteKey(f.host, f.port) === key)
  if (index >= 0) {
    favorites.splice(index, 1)
    saveRemotePcFavorites(favorites)
    return favorites
  }
  return addRemotePcFavorite(name, host, port)
}

export function removeRemotePcFavorite(host: string, port: number): RemotePcFavorite[] {
  const key = favoriteKey(host, port)
  const favorites = loadRemotePcFavorites().filter((f) => favoriteKey(f.host, f.port) !== key)
  saveRemotePcFavorites(favorites)
  return favorites
}
