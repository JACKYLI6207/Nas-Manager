import { loadRemotePcFavorites } from './remotePcFavoritesStorage'
import { looksLikeTailscaleHost, normalizePcName } from './remotePcName'

const STORAGE_KEY = 'gmRemotePcHostRegistry-v1'

type RegistryEntry = {
  /** normalizePcName 後的鍵 */
  pcKey: string
  port: number
  hosts: string[]
  /** 最近一次使用的顯示名（可含區網／偽區網後綴） */
  displayName: string
  updatedAt: number
}

type RegistryMap = Record<string, RegistryEntry>

function entryKey(pcKey: string, port: number): string {
  return `${pcKey}:${port}`
}

function readAll(): RegistryMap {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return {}
    const parsed: unknown = JSON.parse(raw)
    if (typeof parsed !== 'object' || parsed === null) return {}
    return parsed as RegistryMap
  } catch {
    return {}
  }
}

function writeAll(data: RegistryMap) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(data))
  } catch {
    /* ignore */
  }
}

function mergeHosts(existing: string[], incoming: string[]): string[] {
  const out: string[] = []
  const seen = new Set<string>()
  for (const h of [...incoming, ...existing]) {
    const t = h.trim().toLowerCase()
    if (!t || seen.has(t)) continue
    seen.add(t)
    out.push(h.trim())
  }
  return out.slice(0, 16)
}

/** 記錄某台 PC 曾出現過的 host（掃描／連線／收藏時呼叫）。 */
export function rememberRemotePcHosts(
  displayName: string,
  hosts: string[],
  port: number,
): void {
  const cleaned = hosts.map((h) => h.trim()).filter(Boolean)
  if (cleaned.length === 0) return
  const pcKey = normalizePcName(displayName)
  if (!pcKey) return
  const all = readAll()
  const key = entryKey(pcKey, port)
  const prev = all[key]
  all[key] = {
    pcKey,
    port,
    hosts: mergeHosts(prev?.hosts ?? [], cleaned),
    displayName: displayName.trim() || prev?.displayName || pcKey,
    updatedAt: Date.now(),
  }
  writeAll(all)
}

/** 依顯示名（可含區網後綴）取出曾記錄的所有 host。 */
export function hostsForPcName(displayName: string, port: number): string[] {
  const pcKey = normalizePcName(displayName)
  if (!pcKey) return []
  const entry = readAll()[entryKey(pcKey, port)]
  return entry?.hosts ?? []
}

/**
 * 依目前 job 的 host／pcName，收集所有可能可達的 host（原 host + 同名 PC 登錄 + 同名收藏）。
 * 排序：原 host 優先，其次區網私網，再 Tailscale。
 */
export function candidateHostsForStreamJob(job: {
  host: string
  port: number
  pcName?: string | null
}): string[] {
  const primary = job.host.trim()
  const names = new Set<string>()
  if (job.pcName?.trim()) names.add(job.pcName.trim())

  const registry = readAll()
  const favs = loadRemotePcFavorites()

  // 由 host 反查：登錄或收藏裡出現過此 host 的 PC
  for (const entry of Object.values(registry)) {
    if (entry.port !== job.port) continue
    if (entry.hosts.some((h) => h.trim().toLowerCase() === primary.toLowerCase())) {
      names.add(entry.displayName)
    }
  }
  for (const fav of favs) {
    if (fav.port !== job.port) continue
    if (fav.host.trim().toLowerCase() === primary.toLowerCase()) {
      names.add(fav.name)
    }
  }

  const hosts = new Set<string>()
  if (primary) hosts.add(primary)

  for (const name of names) {
    for (const h of hostsForPcName(name, job.port)) hosts.add(h)
    for (const fav of favs) {
      if (fav.port !== job.port) continue
      if (normalizePcName(fav.name) === normalizePcName(name)) {
        hosts.add(fav.host.trim())
      }
    }
  }

  const list = [...hosts].filter(Boolean)
  list.sort((a, b) => {
    if (a === primary) return -1
    if (b === primary) return 1
    const at = looksLikeTailscaleHost(a)
    const bt = looksLikeTailscaleHost(b)
    if (at !== bt) return at ? 1 : -1
    return a.localeCompare(b)
  })
  return list
}
