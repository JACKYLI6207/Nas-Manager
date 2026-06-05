import type { SourceReadRecord } from './localReadStore'
import { comicJobKey, type RemoteComicStreamJob } from './comicStreamStore'

const PROGRESS_KEY = 'gmComicStreamProgress-v1'

type ProgressMap = Record<string, SourceReadRecord>

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

function normalize(rec: Record<string, unknown> | undefined): SourceReadRecord {
  if (!rec) {
    return { opened: false, readPage: 0, totalPages: 0, scrollY: 0, offsetInPage: 0, offsetRatioInPage: 0 }
  }
  return {
    opened: Boolean(rec.opened),
    readPage: typeof rec.readPage === 'number' ? rec.readPage : 0,
    totalPages: typeof rec.totalPages === 'number' ? rec.totalPages : 0,
    scrollY: typeof rec.scrollY === 'number' ? rec.scrollY : 0,
    offsetInPage: typeof rec.offsetInPage === 'number' ? rec.offsetInPage : 0,
    offsetRatioInPage: typeof rec.offsetRatioInPage === 'number' ? rec.offsetRatioInPage : 0,
  }
}

export function getComicStreamRecord(job: RemoteComicStreamJob | null): SourceReadRecord {
  if (!job) {
    return normalize(undefined)
  }
  return normalize(readAll()[comicJobKey(job)] as Record<string, unknown> | undefined)
}

export function saveComicStreamPosition(
  job: RemoteComicStreamJob,
  readPage: number,
  totalPages: number,
  scrollY: number,
  offsetInPage = 0,
  offsetRatioInPage = 0,
) {
  const key = comicJobKey(job)
  const prev = getComicStreamRecord(job)
  const all = readAll()
  all[key] = {
    opened: true,
    readPage: Math.max(0, readPage),
    totalPages: totalPages > 0 ? totalPages : prev.totalPages,
    scrollY: Math.max(0, scrollY),
    offsetInPage: Math.max(0, offsetInPage),
    offsetRatioInPage: Math.min(1, Math.max(0, offsetRatioInPage)),
  }
  writeAll(all)
}

export function markComicStreamOpened(job: RemoteComicStreamJob, totalPages: number) {
  const prev = getComicStreamRecord(job)
  const all = readAll()
  all[comicJobKey(job)] = {
    ...prev,
    opened: true,
    totalPages: totalPages > 0 ? totalPages : prev.totalPages,
  }
  writeAll(all)
}

export function formatComicStreamProgressLabel(job: RemoteComicStreamJob | null): string | null {
  const rec = getComicStreamRecord(job)
  if (!rec.opened || rec.totalPages <= 0) return null
  const page = Math.min(rec.readPage + 1, rec.totalPages)
  return `${page}/${rec.totalPages}頁`
}

export function clearComicStreamRecord(job: RemoteComicStreamJob) {
  const all = readAll()
  delete all[comicJobKey(job)]
  writeAll(all)
}

export function hasComicStreamRecord(job: RemoteComicStreamJob): boolean {
  return getComicStreamRecord(job).opened
}
