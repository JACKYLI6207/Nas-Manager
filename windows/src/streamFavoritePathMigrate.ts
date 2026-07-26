/**
 * 多分享根時，PC 現以 Volume GUID 當 API 根（vol_<guid>…），
 * 舊收藏可能仍是磁碟代號衍生 label（如 H--）。
 * 連上 PC 後用 health.shareMounts 的 legacyLabels → label 對照改寫，避免收藏失效。
 */

export type ShareMountInfo = {
  label: string
  display?: string
  volumeGuid?: string
  legacyLabels?: string[]
}

function normRel(p: string): string {
  return p.replace(/\\/g, '/').replace(/^\/+/, '')
}

/** 若 head 對應到某 mount 的 legacy／舊 label，改寫為目前主 label */
export function rewriteRelPathWithMountMap(relPath: string, mounts: ShareMountInfo[]): string {
  if (!mounts.length) return relPath
  const norm = normRel(relPath)
  if (!norm) return relPath
  const slash = norm.indexOf('/')
  const head = slash >= 0 ? norm.slice(0, slash) : norm
  const rest = slash >= 0 ? norm.slice(slash + 1) : ''

  for (const m of mounts) {
    if (!m.label) continue
    const aliases = new Set<string>([
      m.label,
      ...(m.legacyLabels ?? []),
    ])
    if (m.volumeGuid?.trim()) {
      const g = m.volumeGuid.trim().replace(/^\{|\}$/g, '')
      aliases.add(g)
      aliases.add(`vol_${g.toLowerCase()}`)
      aliases.add(`{${g}}`)
    }
    const hit = [...aliases].some((a) => a === head || a.toLowerCase() === head.toLowerCase())
    if (!hit) continue
    if (head === m.label) return norm
    return rest ? `${m.label}/${rest}` : m.label
  }
  return norm
}

export function migrateJobsRelPaths<T extends { relPath: string }>(
  jobs: T[],
  mounts: ShareMountInfo[],
): { jobs: T[]; changed: boolean } {
  let changed = false
  const next = jobs.map((j) => {
    const rewritten = rewriteRelPathWithMountMap(j.relPath, mounts)
    if (rewritten !== j.relPath) {
      changed = true
      return { ...j, relPath: rewritten }
    }
    return j
  })
  return { jobs: next, changed }
}
