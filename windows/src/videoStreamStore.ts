import { reactive } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { jobKey } from './streamPlaylistStorage'

export type RemoteStreamJob = {
  host: string
  port: number
  relPath: string
  title: string
}

export const videoStreamSession = reactive({
  /** 待播放佇列（不含 currentJob） */
  jobs: [] as RemoteStreamJob[],
  logLines: [] as string[],
  active: false,
  currentJob: null as RemoteStreamJob | null,
  navigateSeq: 0,
  playbackLocked: false,
})

export function streamLog(line: string) {
  const ts = new Date().toLocaleString('zh-TW', { hour12: false })
  videoStreamSession.logLines = [...videoStreamSession.logLines, `[${ts}] ${line}`]
}

export function clearStreamLog() {
  videoStreamSession.logLines = []
}

/** 完整列表：載入順序固定，除非清空列表否則不重排 */
export function getFullStreamQueue(): RemoteStreamJob[] {
  return [...videoStreamSession.jobs]
}

export function queueRemoteStreamPlay(jobs: RemoteStreamJob[]) {
  clearStreamLog()
  videoStreamSession.jobs = [...jobs]
  videoStreamSession.active = jobs.length > 0
  videoStreamSession.currentJob = null
  releaseStreamPlaybackLock()
  streamLog(`已加入播放列表：${jobs.length} 部影片`)
  for (const j of jobs) {
    streamLog(`  · ${j.relPath}`)
  }
  void syncNativeStreamPlaylist()
}

export function requestNavigateToVideoTab() {
  videoStreamSession.navigateSeq += 1
}

export function acquireStreamPlaybackLock(): boolean {
  if (videoStreamSession.playbackLocked) return false
  videoStreamSession.playbackLocked = true
  return true
}

export function releaseStreamPlaybackLock() {
  videoStreamSession.playbackLocked = false
}

export function shiftNextStreamJob(): RemoteStreamJob | undefined {
  const list = videoStreamSession.jobs
  if (list.length === 0) return undefined
  const cur = videoStreamSession.currentJob
  if (!cur) return list[0]
  const key = jobKey(cur)
  const idx = list.findIndex((j) => jobKey(j) === key)
  if (idx < 0) return list[0]
  return list[idx + 1]
}

export function setCurrentStreamJob(job: RemoteStreamJob | null) {
  videoStreamSession.currentJob = job ? { ...job } : null
  void syncNativeStreamPlaylist()
}

export function finishStreamSession() {
  videoStreamSession.active = false
  videoStreamSession.jobs = []
  videoStreamSession.currentJob = null
  releaseStreamPlaybackLock()
  void syncNativeStreamPlaylist()
}

/** 清空當前播放列表（不影響收藏列表） */
export function clearCurrentStreamQueue() {
  finishStreamSession()
  streamLog('已清空當前播放列表')
}

export function removeFromCurrentQueue(job: RemoteStreamJob) {
  const key = jobKey(job)
  videoStreamSession.jobs = videoStreamSession.jobs.filter((j) => jobKey(j) !== key)
  if (videoStreamSession.currentJob && jobKey(videoStreamSession.currentJob) === key) {
    videoStreamSession.currentJob = null
  }
  if (!videoStreamSession.currentJob && videoStreamSession.jobs.length === 0) {
    videoStreamSession.active = false
  }
  void syncNativeStreamPlaylist()
}

/**
 * 從列表點選：只更新 current，不重排 jobs 順序。
 * sourceJobs 有值時整份替換為收藏列表順序。
 */
export function activateStreamQueueJob(
  target: RemoteStreamJob,
  sourceJobs?: RemoteStreamJob[],
) {
  if (sourceJobs != null) {
    videoStreamSession.jobs = sourceJobs.map((j) => ({ ...j }))
  } else {
    const key = jobKey(target)
    if (!videoStreamSession.jobs.some((j) => jobKey(j) === key)) {
      videoStreamSession.jobs = [{ ...target }]
    }
  }
  videoStreamSession.currentJob = { ...target }
  videoStreamSession.active = true
  streamLog(`切換播放：${target.title}`)
  void syncNativeStreamPlaylist()
  requestNavigateToVideoTab()
}

export async function syncNativeStreamPlaylist() {
  try {
    const jobs = getFullStreamQueue()
    await invoke('sync_stream_playlist', {
      jobs,
      currentRelPath: videoStreamSession.currentJob?.relPath ?? jobs[0]?.relPath ?? null,
    })
  } catch {
    // 桌面版或未載入 plugin 時略過
  }
}
