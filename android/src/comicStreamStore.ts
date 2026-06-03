import { reactive, ref } from 'vue'
import type { RemoteStreamJob } from './videoStreamStore'
import { comicReadSubTab } from './comicReadUi'

/** 與影片串流相同結構：host + port + relPath + title */
export type RemoteComicStreamJob = RemoteStreamJob

export const comicStreamSession = reactive({
  jobs: [] as RemoteComicStreamJob[],
  logLines: [] as string[],
  active: false,
  currentJob: null as RemoteComicStreamJob | null,
  navigateSeq: 0,
  /** 切換到串流閱讀子分頁後自動開讀 */
  pendingRead: false,
})

export const comicStreamPageSrcMap = ref(new Map<number, string>())
export const comicStreamFailedIndices = ref(new Set<number>())

export const comicStreamReadSession = reactive({
  readerPages: [] as { index: number; caption: string; entry: string }[],
  readerTitle: '',
  readingActive: false,
  openingBusy: false,
  status: '',
})

export function comicStreamLog(line: string) {
  const ts = new Date().toLocaleString('zh-TW', { hour12: false })
  comicStreamSession.logLines = [...comicStreamSession.logLines, `[${ts}] ${line}`]
}

export function clearComicStreamLog() {
  comicStreamSession.logLines = []
}

export function getFullComicStreamQueue(): RemoteComicStreamJob[] {
  return [...comicStreamSession.jobs]
}

export function queueRemoteComicStreamPlay(jobs: RemoteComicStreamJob[]) {
  clearComicStreamLog()
  comicStreamSession.jobs = [...jobs]
  comicStreamSession.active = jobs.length > 0
  comicStreamSession.currentJob = null
  comicStreamLog(`已加入串流列表：${jobs.length} 本漫畫`)
  for (const j of jobs) {
    comicStreamLog(`  · ${j.relPath}`)
  }
}

export function requestNavigateToComicStream() {
  comicStreamSession.navigateSeq += 1
}

export function setCurrentComicStreamJob(job: RemoteComicStreamJob | null) {
  comicStreamSession.currentJob = job ? { ...job } : null
}

export function shiftNextComicStreamJob(): RemoteComicStreamJob | undefined {
  const list = comicStreamSession.jobs
  if (list.length === 0) return undefined
  const cur = comicStreamSession.currentJob
  if (!cur) return list[0]
  const key = comicJobKey(cur)
  const idx = list.findIndex((j) => comicJobKey(j) === key)
  if (idx < 0) return list[0]
  return list[idx + 1]
}

export function finishComicStreamSession() {
  comicStreamSession.active = false
  comicStreamSession.jobs = []
  comicStreamSession.currentJob = null
}

export function clearCurrentComicStreamQueue() {
  finishComicStreamSession()
  comicStreamLog('已清空串流列表')
}

export function removeFromComicStreamQueue(job: RemoteComicStreamJob) {
  const key = comicJobKey(job)
  comicStreamSession.jobs = comicStreamSession.jobs.filter((j) => comicJobKey(j) !== key)
  if (comicStreamSession.currentJob && comicJobKey(comicStreamSession.currentJob) === key) {
    comicStreamSession.currentJob = null
  }
  if (!comicStreamSession.currentJob && comicStreamSession.jobs.length === 0) {
    comicStreamSession.active = false
  }
}

export function comicJobKey(job: RemoteComicStreamJob): string {
  return `${job.host.trim().toLowerCase()}:${job.port}:${job.relPath}`
}

/** 從列表點選：只更新 current，不重排 jobs 順序。sourceJobs 有值時整份替換。 */
export function activateComicStreamQueueJob(
  target: RemoteComicStreamJob,
  sourceJobs?: RemoteComicStreamJob[],
) {
  if (sourceJobs != null) {
    comicStreamSession.jobs = sourceJobs.map((j) => ({ ...j }))
  } else {
    const key = comicJobKey(target)
    if (!comicStreamSession.jobs.some((j) => comicJobKey(j) === key)) {
      comicStreamSession.jobs = [{ ...target }]
    }
  }
  comicStreamSession.currentJob = { ...target }
  comicStreamSession.active = true
  comicStreamSession.pendingRead = true
  comicReadSubTab.value = 'home'
  comicStreamLog(`切換閱讀：${target.title}`)
  requestNavigateToComicStream()
}

export function resetComicStreamReaderUi() {
  comicStreamPageSrcMap.value.forEach((url) => URL.revokeObjectURL(url))
  comicStreamPageSrcMap.value = new Map()
  comicStreamFailedIndices.value = new Set()
  comicStreamReadSession.readerPages = []
  comicStreamReadSession.readerTitle = ''
  comicStreamReadSession.readingActive = false
  comicStreamReadSession.openingBusy = false
  comicStreamReadSession.status = ''
}
