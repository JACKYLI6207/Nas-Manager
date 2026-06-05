import { jobKey } from './streamPlaylistStorage'
import type { RemoteStreamJob } from './videoStreamStore'

const PROGRESS_KEY = 'gmVideoStreamProgress-v1'

export type VideoStreamRecord = {
  opened: boolean
  positionMs: number
  durationMs: number
}

type ProgressMap = Record<string, VideoStreamRecord>

function readAll(): ProgressMap {
  try {
    const raw = localStorage.getItem(PROGRESS_KEY)
    return raw ? (JSON.parse(raw) as ProgressMap) : {}
  } catch {
    return {}
  }
}

function writeAll(data: ProgressMap) {
  try {
    localStorage.setItem(PROGRESS_KEY, JSON.stringify(data))
  } catch {
    /* 容量不足時略過 */
  }
}

function normalize(rec: Record<string, unknown> | undefined): VideoStreamRecord {
  if (!rec) {
    return { opened: false, positionMs: 0, durationMs: 0 }
  }
  return {
    opened: Boolean(rec.opened),
    positionMs: typeof rec.positionMs === 'number' ? Math.max(0, rec.positionMs) : 0,
    durationMs: typeof rec.durationMs === 'number' ? Math.max(0, rec.durationMs) : 0,
  }
}

export function getVideoStreamRecord(job: RemoteStreamJob | null): VideoStreamRecord {
  if (!job) return normalize(undefined)
  return normalize(readAll()[jobKey(job)] as Record<string, unknown> | undefined)
}

export function saveVideoStreamProgress(
  job: RemoteStreamJob,
  positionMs: number,
  durationMs: number,
) {
  const key = jobKey(job)
  const prev = getVideoStreamRecord(job)
  const all = readAll()
  all[key] = {
    opened: true,
    positionMs: Math.max(0, positionMs),
    durationMs: durationMs > 0 ? durationMs : prev.durationMs,
  }
  writeAll(all)
}

export function markVideoStreamOpened(job: RemoteStreamJob) {
  const prev = getVideoStreamRecord(job)
  const all = readAll()
  all[jobKey(job)] = { ...prev, opened: true }
  writeAll(all)
}

export function hasVideoStreamRecord(job: RemoteStreamJob): boolean {
  return getVideoStreamRecord(job).opened
}

export function videoStreamProgressRatio(job: RemoteStreamJob): number | null {
  const rec = getVideoStreamRecord(job)
  if (!rec.opened || rec.positionMs <= 0) return null
  if (rec.durationMs > 0) {
    return Math.min(1, Math.max(0, rec.positionMs / rec.durationMs))
  }
  return null
}

function formatClockMs(ms: number): string {
  const totalSec = Math.max(0, Math.floor(ms / 1000))
  const h = Math.floor(totalSec / 3600)
  const m = Math.floor((totalSec % 3600) / 60)
  const s = totalSec % 60
  const mm = String(m).padStart(2, '0')
  const ss = String(s).padStart(2, '0')
  if (h > 0) return `${h}:${mm}:${ss}`
  return `${m}:${ss}`
}

export function formatVideoStreamProgressLabel(job: RemoteStreamJob | null): string | null {
  const rec = getVideoStreamRecord(job)
  if (!rec.opened) return null
  if (rec.positionMs <= 0) return '已觀看'
  if (rec.durationMs > 0) {
    return `${formatClockMs(rec.positionMs)} / ${formatClockMs(rec.durationMs)}`
  }
  return formatClockMs(rec.positionMs)
}
