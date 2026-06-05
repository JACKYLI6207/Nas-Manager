<script setup lang="ts">
import { computed, nextTick, onActivated, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  getBackgroundPlaybackSession,
  getMobileSettings,
  listVideoPlayerOptions,
  listenVideoStreamPrepProgress,
  pickExternalVideoPlayerExe,
  pickLocalVideoFile,
  playLocalVideoFile,
  playRemotePcVideo,
  setDefaultVideoPlayer,
  stopVideoPlayback,
  SYSTEM_DEFAULT_PLAYER_ID,
  type BackgroundPlaybackSession,
  type PlayVideoResult,
  type VideoPlayerOption,
  type VideoStreamPrepProgressEvent,
} from '../api'
import { copyTextFromTextarea } from '../copyText'
import { formatInvokeError } from '../invokeError'
import {
  addFavoriteStreamPlaylist,
  jobKey,
  loadFavoriteStreamPlaylists,
  removeFavoriteStreamPlaylist,
  removeFavoriteStreamPlaylistItem,
  renameFavoriteStreamPlaylist,
  type FavoriteStreamPlaylist,
} from '../streamPlaylistStorage'
import StreamPlaylistList from './StreamPlaylistList.vue'
import {
  formatVideoStreamProgressLabel,
  hasVideoStreamRecord,
  markVideoStreamOpened,
  videoStreamProgressRatio,
} from '../videoStreamProgress'
import {
  acquireStreamPlaybackLock,
  activateStreamQueueJob,
  clearCurrentStreamQueue,
  finishStreamSession,
  getFullStreamQueue,
  releaseStreamPlaybackLock,
  requestNavigateToVideoTab,
  removeFromCurrentQueue,
  setCurrentStreamJob,
  shiftNextStreamJob,
  streamLog,
  syncNativeStreamPlaylist,
  videoStreamSession,
  type RemoteStreamJob,
} from '../videoStreamStore'
import '../readerShared.css'

const busy = ref(false)
const awaitingSystemPicker = ref(false)
const status = ref('')
const backgroundSession = ref<BackgroundPlaybackSession | null>(null)
const streamLogRef = ref<HTMLTextAreaElement | null>(null)
const copyLogHint = ref('')

let pickerVisibilityTimer: ReturnType<typeof setTimeout> | undefined
let pickerSafetyTimer: ReturnType<typeof setTimeout> | undefined
let pickerWentHidden = false
let pickGeneration = 0
let pickMode: 'video' | null = null
const streamLogText = computed(() => videoStreamSession.logLines.join('\n'))
const hasStreamLog = computed(() => videoStreamSession.logLines.length > 0)
const streamQueueCount = computed(() => videoStreamSession.jobs.length)
const fullStreamQueue = computed(() => getFullStreamQueue())
const fullQueueCount = computed(() => fullStreamQueue.value.length)
const currentStreamJob = computed(() => videoStreamSession.currentJob)
const currentJobKey = computed(() =>
  currentStreamJob.value ? jobKey(currentStreamJob.value) : null,
)

type VideoSubTab = 'home' | 'current' | 'favorites' | 'log'

const videoSubTabs: { key: VideoSubTab; label: string }[] = [
  { key: 'home', label: '播放主頁' },
  { key: 'current', label: '串流列表' },
  { key: 'favorites', label: '串流收藏' },
  { key: 'log', label: '串流日誌' },
]

const videoSubTab = ref<VideoSubTab>('home')
const favoritePlaylists = ref<FavoriteStreamPlaylist[]>([])
const expandedFavoriteIds = ref<Set<string>>(new Set())
const videoProgressTick = ref(0)

function videoProgressLabel(job: RemoteStreamJob): string | null {
  void videoProgressTick.value
  return formatVideoStreamProgressLabel(job)
}

function videoOpenedMark(job: RemoteStreamJob): boolean {
  void videoProgressTick.value
  return hasVideoStreamRecord(job)
}

function videoProgressRatioForJob(job: RemoteStreamJob): number | null {
  void videoProgressTick.value
  return videoStreamProgressRatio(job)
}

function bumpVideoProgressUi() {
  videoProgressTick.value += 1
}

const playerOptions = ref<VideoPlayerOption[]>([])
const selectedPlayer = ref<VideoPlayerOption | null>(null)
const playerPickerBusy = ref(false)
const playerMenuOpen = ref(false)
const playerDropdownRef = ref<HTMLElement | null>(null)
const streamPrepProgress = ref<VideoStreamPrepProgressEvent | null>(null)
const streamPrepTitle = ref('')

let streamPrepUnlisten: (() => void) | null = null

const otherPlayerOptions = computed(() => {
  const currentId = selectedPlayer.value?.id
  return playerOptions.value.filter((p) => p.id !== currentId)
})

function togglePlayerMenu() {
  playerMenuOpen.value = !playerMenuOpen.value
}

function closePlayerMenu() {
  playerMenuOpen.value = false
}

function formatSize(size: number): string {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  if (size < 1024 * 1024 * 1024) return `${(size / (1024 * 1024)).toFixed(1)} MB`
  return `${(size / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function formatSpeed(bps: number): string {
  if (bps < 1024) return `${bps} B/s`
  if (bps < 1024 * 1024) return `${(bps / 1024).toFixed(1)} KB/s`
  return `${(bps / (1024 * 1024)).toFixed(1)} MB/s`
}

function beginStreamPrepOverlay(title: string) {
  streamPrepTitle.value = title
  streamPrepProgress.value = {
    phase: 'checking',
    message: '正在準備串流播放…',
    bytesDone: 0,
    bytesTotal: 0,
    speedBps: 0,
    finished: false,
    error: null,
  }
}

function endStreamPrepOverlay() {
  streamPrepProgress.value = null
  streamPrepTitle.value = ''
}

function onDocumentClickForPlayerMenu(e: MouseEvent) {
  if (!playerMenuOpen.value) return
  const el = playerDropdownRef.value
  if (el && !el.contains(e.target as Node)) {
    closePlayerMenu()
  }
}

async function loadVideoPlayerSettings() {
  try {
    playerOptions.value = await listVideoPlayerOptions()
    const savedPath = (await getMobileSettings()).externalVideoPlayerPath?.trim()
    if (savedPath) {
      const normalized = savedPath.replace(/\\/g, '/').toLowerCase()
      selectedPlayer.value =
        playerOptions.value.find(
          (p) =>
            p.exePath?.replace(/\\/g, '/').toLowerCase() === normalized,
        ) ?? (await setDefaultVideoPlayer(savedPath))
    } else {
      selectedPlayer.value =
        playerOptions.value.find((p) => p.id === SYSTEM_DEFAULT_PLAYER_ID) ?? null
    }
  } catch {
    selectedPlayer.value = {
      id: SYSTEM_DEFAULT_PLAYER_ID,
      name: '系統預設',
      exePath: null,
    }
  }
}

async function chooseVideoPlayer(option: VideoPlayerOption) {
  if (playerPickerBusy.value) return
  playerPickerBusy.value = true
  try {
    selectedPlayer.value = await setDefaultVideoPlayer(option.exePath)
    status.value = `已選擇：${selectedPlayer.value.name}`
    closePlayerMenu()
  } catch (e) {
    status.value = formatInvokeError(e)
  } finally {
    playerPickerBusy.value = false
  }
}

async function browseVideoPlayerExe() {
  if (playerPickerBusy.value) return
  playerPickerBusy.value = true
  try {
    const picked = await pickExternalVideoPlayerExe()
    if (!picked) return
    selectedPlayer.value = await setDefaultVideoPlayer(picked)
    playerOptions.value = await listVideoPlayerOptions()
    status.value = `已選擇：${selectedPlayer.value.name}`
    closePlayerMenu()
  } catch (e) {
    status.value = formatInvokeError(e)
  } finally {
    playerPickerBusy.value = false
  }
}

function reloadFavoritePlaylists() {
  favoritePlaylists.value = loadFavoriteStreamPlaylists()
}

function setVideoSubTab(tab: VideoSubTab) {
  videoSubTab.value = tab
  if (tab === 'favorites') {
    reloadFavoritePlaylists()
  }
  if (tab === 'current' || tab === 'favorites') {
    bumpVideoProgressUi()
  }
}

function isFavoriteExpanded(id: string): boolean {
  return expandedFavoriteIds.value.has(id)
}

function toggleFavoriteExpanded(id: string) {
  const next = new Set(expandedFavoriteIds.value)
  if (next.has(id)) {
    next.delete(id)
  } else {
    next.add(id)
  }
  expandedFavoriteIds.value = next
}
const canResumeStream = computed(
  () =>
    Boolean(currentStreamJob.value) &&
    !videoStreamSession.playbackLocked &&
    !busy.value &&
    !awaitingSystemPicker.value,
)
const canResumeBackground = computed(
  () =>
    Boolean(backgroundSession.value) &&
    !videoStreamSession.playbackLocked &&
    !busy.value &&
    !awaitingSystemPicker.value,
)
const canShowResume = computed(
  () =>
    (canResumeBackground.value || canResumeStream.value) &&
    !videoStreamSession.playbackLocked &&
    !busy.value &&
    !awaitingSystemPicker.value,
)
const resumeTitle = computed(() => {
  if (backgroundSession.value?.title) return backgroundSession.value.title
  return currentStreamJob.value?.title ?? ''
})

function parseBackgroundSession(raw: string | null): BackgroundPlaybackSession | null {
  if (!raw?.trim()) return null
  try {
    const data = JSON.parse(raw) as Partial<BackgroundPlaybackSession>
    if (!data.uri?.trim()) return null
    return {
      uri: data.uri,
      title: data.title?.trim() || '背景播放中',
      pcHost: data.pcHost ?? '',
      pcPort: Number(data.pcPort ?? 0),
      pcRelPath: data.pcRelPath ?? '',
      subtitleUris: Array.isArray(data.subtitleUris) ? data.subtitleUris : [],
    }
  } catch {
    return null
  }
}

async function refreshBackgroundSession() {
  try {
    const raw = await getBackgroundPlaybackSession()
    backgroundSession.value = parseBackgroundSession(raw)
  } catch {
    backgroundSession.value = null
  }
}

async function handlePlayVideoResult(
  result: PlayVideoResult,
  options?: { preserveSession?: boolean },
): Promise<string | null> {
  if (result.background) {
    await refreshBackgroundSession()
    status.value = '已暫停，可點「繼續播放」回到播放器'
    return null
  }
  if (!options?.preserveSession) {
    backgroundSession.value = null
  }
  return result.error?.trim() || null
}

function clearPickerTimers() {
  if (pickerVisibilityTimer !== undefined) {
    clearTimeout(pickerVisibilityTimer)
    pickerVisibilityTimer = undefined
  }
  if (pickerSafetyTimer !== undefined) {
    clearTimeout(pickerSafetyTimer)
    pickerSafetyTimer = undefined
  }
}

function releaseBusyState() {
  busy.value = false
  awaitingSystemPicker.value = false
  pickMode = null
  pickGeneration += 1
}

function onDocumentVisibilityChange() {
  if (!awaitingSystemPicker.value) return
  if (document.visibilityState === 'hidden') {
    pickerWentHidden = true
    if (pickerVisibilityTimer !== undefined) {
      clearTimeout(pickerVisibilityTimer)
      pickerVisibilityTimer = undefined
    }
    return
  }
  if (!pickerWentHidden) return
  if (pickerVisibilityTimer !== undefined) clearTimeout(pickerVisibilityTimer)
  pickerVisibilityTimer = window.setTimeout(() => {
    pickerVisibilityTimer = undefined
    if (awaitingSystemPicker.value) {
      cancelPickerWait('選擇未完成，請再試一次')
    }
  }, 12000)
}

function beginPickerWait() {
  pickerWentHidden = false
  clearPickerTimers()
  document.addEventListener('visibilitychange', onDocumentVisibilityChange)
  pickerSafetyTimer = window.setTimeout(() => {
    pickerSafetyTimer = undefined
    if (awaitingSystemPicker.value) {
      cancelPickerWait('選擇逾時，請再試一次')
    }
  }, 5 * 60 * 1000)
}

function endPickerWait() {
  clearPickerTimers()
  document.removeEventListener('visibilitychange', onDocumentVisibilityChange)
}

function cancelPickerWait(message?: string) {
  endPickerWait()
  releaseBusyState()
  status.value = message ?? ''
}

function promiseWithTimeout<T>(promise: Promise<T>, ms: number, message: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error(message)), ms)
    promise
      .then((v) => {
        clearTimeout(timer)
        resolve(v)
      })
      .catch((e) => {
        clearTimeout(timer)
        reject(e)
      })
  })
}

async function pickWithOverlay(picker: () => Promise<string | null>): Promise<string | null> {
  if (busy.value) return null
  const gen = ++pickGeneration
  pickMode = 'video'
  busy.value = true
  awaitingSystemPicker.value = true
  status.value = '請在系統選擇器挑選影片…'
  beginPickerWait()
  try {
    const uri = await promiseWithTimeout(picker(), 5 * 60 * 1000, '選擇逾時，請再試一次')
    endPickerWait()
    awaitingSystemPicker.value = false
    if (gen !== pickGeneration) return null
    return uri
  } catch (e) {
    endPickerWait()
    status.value = formatInvokeError(e)
    return null
  } finally {
    if (gen === pickGeneration) {
      busy.value = false
      pickMode = null
    }
  }
}

async function openLocalVideo() {
  const uri = await pickWithOverlay(pickLocalVideoFile)
  if (!uri) return
  status.value = '正在開啟播放器…'
  busy.value = true
  try {
    const result = await playLocalVideoFile(uri)
    const playbackErr = await handlePlayVideoResult(result)
    if (playbackErr) {
      status.value = `播放錯誤：${playbackErr}`
    } else if (!result.background) {
      status.value = ''
    }
  } catch (e) {
    status.value = formatInvokeError(e)
  } finally {
    busy.value = false
  }
}

async function playStreamJob(job: RemoteStreamJob) {
  if (!acquireStreamPlaybackLock()) return
  setCurrentStreamJob(job)
  markVideoStreamOpened(job)
  bumpVideoProgressUi()
  await syncNativeStreamPlaylist()
  status.value = `正在串流：${job.title}`
  streamLog(`開始播放：${job.relPath}`)
  streamLog(`  PC ${job.host}:${job.port}`)
  beginStreamPrepOverlay(job.title)
  let pausedInBackground = false
  let playbackErr: string | null | undefined
  try {
    const result = await playRemotePcVideo(
      job.host,
      job.port,
      job.relPath,
      job.title,
    )
    pausedInBackground = result.background === true
    playbackErr = await handlePlayVideoResult(result)
    if (playbackErr) {
      streamLog(`播放錯誤：${job.title}`)
      streamLog(`  ${playbackErr}`)
      if (playbackErr.includes('DECODING')) {
        streamLog('  提示：x265/10bit MKV 可試「外部播放」(VLC) 或請 PC 轉 H.264')
      }
      status.value = `播放錯誤：${playbackErr}`
    } else if (result.background) {
      streamLog(`背景播放中：${job.title}`)
    } else {
      streamLog(`已結束播放：${job.title}`)
      const remain = videoStreamSession.jobs.length
      status.value =
        remain > 0
          ? `已播放 ${job.title}，播放列表尚餘 ${remain} 部`
          : `已播放 ${job.title}`
    }
  } catch (e) {
    const err = formatInvokeError(e)
    streamLog(`失敗：${job.title}`)
    streamLog(`  ${err}`)
    status.value = err
    playbackErr = err
  } finally {
    releaseStreamPlaybackLock()
    window.setTimeout(
      () => endStreamPrepOverlay(),
      streamPrepProgress.value?.error ? 1200 : 350,
    )
    if (!pausedInBackground && !playbackErr && !backgroundSession.value) {
      const next = shiftNextStreamJob()
      if (next) {
        setCurrentStreamJob(next)
      } else {
        setCurrentStreamJob(null)
        videoStreamSession.active = false
      }
    }
    if (videoStreamSession.jobs.length === 0 && !videoStreamSession.currentJob) {
      videoStreamSession.active = false
    }
  }
}

async function playNextRemoteStreamJob() {
  if (videoStreamSession.playbackLocked) return
  const cur = videoStreamSession.currentJob
  if (cur) {
    const next = shiftNextStreamJob()
    if (!next) {
      streamLog('播放列表已播完')
      finishStreamSession()
      status.value = '播放列表已播完'
      return
    }
    setCurrentStreamJob(next)
    await playStreamJob(next)
    return
  }
  await processRemoteStreamQueue()
}

async function onPickQueueJob(job: RemoteStreamJob) {
  if (videoStreamSession.playbackLocked) {
    try {
      await stopVideoPlayback()
    } catch {
      /* ignore */
    }
    releaseStreamPlaybackLock()
    backgroundSession.value = null
  }
  activateStreamQueueJob(job)
  await nextTick()
  await processRemoteStreamQueue()
}

async function onPickFavoriteQueueJob(pl: FavoriteStreamPlaylist, job: RemoteStreamJob) {
  if (videoStreamSession.playbackLocked) {
    try {
      await stopVideoPlayback()
    } catch {
      /* ignore */
    }
    releaseStreamPlaybackLock()
    backgroundSession.value = null
  }
  activateStreamQueueJob(job, pl.jobs)
  await nextTick()
  await processRemoteStreamQueue()
}

function onClearCurrentPlaylist() {
  if (!window.confirm('確定清空串流列表？')) return
  void stopPlaybackCompletely()
  clearCurrentStreamQueue()
  status.value = '已清空串流列表'
}

function onStarCurrentPlaylist() {
  const jobs = getFullStreamQueue()
  if (jobs.length === 0) {
    status.value = '播放列表為空'
    return
  }
  const suggested = `播放列表 ${jobs.length} 部`
  const name = window.prompt('為此播放列表命名', suggested)
  if (name === null) return
  favoritePlaylists.value = addFavoriteStreamPlaylist(name, jobs)
  streamLog(`已加入收藏播放列表：${name.trim() || suggested}`)
}

function onRenameFavoritePlaylist(pl: FavoriteStreamPlaylist) {
  const name = window.prompt('重新命名播放列表', pl.name)
  if (name === null) return
  favoritePlaylists.value = renameFavoriteStreamPlaylist(pl.id, name)
}

function onDeleteFavoritePlaylist(pl: FavoriteStreamPlaylist) {
  if (!window.confirm(`刪除收藏「${pl.name}」？`)) return
  favoritePlaylists.value = removeFavoriteStreamPlaylist(pl.id)
}

async function onPlayFavoritePlaylist(pl: FavoriteStreamPlaylist) {
  if (pl.jobs.length === 0) return
  if (videoStreamSession.playbackLocked) {
    await stopVideoPlayback()
    releaseStreamPlaybackLock()
  }
  videoStreamSession.jobs = pl.jobs.map((j) => ({ ...j }))
  videoStreamSession.currentJob = null
  videoStreamSession.active = true
  streamLog(`載入串流收藏：${pl.name}（${pl.jobs.length} 部）`)
  requestNavigateToVideoTab()
  await nextTick()
  await processRemoteStreamQueue()
}

async function resumeCurrentStream() {
  const job = currentStreamJob.value
  if (!job || videoStreamSession.playbackLocked) return
  if (!acquireStreamPlaybackLock()) return
  status.value = `正在接續：${job.title}`
  streamLog(`接續觀看：${job.relPath}`)
  beginStreamPrepOverlay(job.title)
  try {
    await syncNativeStreamPlaylist()
    const result = await playRemotePcVideo(
      job.host,
      job.port,
      job.relPath,
      job.title,
      null,
      { resumeOnly: true },
    )
    const playbackErr = await handlePlayVideoResult(result, { preserveSession: true })
    await refreshBackgroundSession()
    if (playbackErr) {
      streamLog(`播放錯誤：${playbackErr}`)
      status.value = `播放錯誤：${playbackErr}`
    } else if (!result.background) {
      status.value = ''
    }
  } catch (e) {
    status.value = formatInvokeError(e)
    streamLog(`接續失敗：${formatInvokeError(e)}`)
  } finally {
    releaseStreamPlaybackLock()
    window.setTimeout(() => endStreamPrepOverlay(), 350)
  }
}

async function resumeBackgroundPlayback() {
  const session = backgroundSession.value
  if (!session || videoStreamSession.playbackLocked) return
  if (!acquireStreamPlaybackLock()) return
  status.value = `正在接續：${session.title}`
  streamLog(`接續背景播放：${session.title}`)
  try {
    let result: PlayVideoResult
    if (session.pcHost && session.pcPort > 0 && session.pcRelPath) {
      beginStreamPrepOverlay(session.title)
      await syncNativeStreamPlaylist()
      result = await playRemotePcVideo(
        session.pcHost,
        session.pcPort,
        session.pcRelPath,
        session.title,
        session.subtitleUris,
        { resumeOnly: true },
      )
    } else {
      result = await playLocalVideoFile(session.uri, session.title, session.subtitleUris, {
        resumeOnly: true,
      })
    }
    const playbackErr = await handlePlayVideoResult(result, { preserveSession: true })
    await refreshBackgroundSession()
    if (playbackErr) {
      streamLog(`播放錯誤：${playbackErr}`)
      status.value = `播放錯誤：${playbackErr}`
    } else if (!result.background) {
      backgroundSession.value = null
      status.value = ''
    }
  } catch (e) {
    status.value = formatInvokeError(e)
    streamLog(`接續失敗：${formatInvokeError(e)}`)
  } finally {
    releaseStreamPlaybackLock()
    window.setTimeout(() => endStreamPrepOverlay(), 350)
  }
}

async function stopPlaybackCompletely() {
  try {
    await stopVideoPlayback()
  } catch (e) {
    status.value = formatInvokeError(e)
    streamLog(`停止播放失敗：${formatInvokeError(e)}`)
    return false
  }
  backgroundSession.value = null
  setCurrentStreamJob(null)
  finishStreamSession()
  return true
}

async function endBackgroundPlayback() {
  const ok = await stopPlaybackCompletely()
  if (ok) {
    status.value = '已結束播放'
    streamLog('已結束背景播放')
  }
}

async function endCurrentStream() {
  const ok = await stopPlaybackCompletely()
  if (ok) {
    status.value = '已結束播放'
    streamLog('已結束串流播放')
  }
}

async function processRemoteStreamQueue() {
  if (!videoStreamSession.active) return
  if (videoStreamSession.playbackLocked) return
  let job = videoStreamSession.currentJob
  if (!job) {
    job = shiftNextStreamJob() ?? null
    if (!job) return
    setCurrentStreamJob(job)
  }
  await playStreamJob(job)
}

function onAppForegroundForPlayback() {
  // 播放器在獨立 Task，MainActivity 會自動切回播放畫面
}

async function copyStreamLog() {
  const text = streamLogText.value
  if (!text.trim()) return
  const ok = await copyTextFromTextarea(text, streamLogRef.value)
  copyLogHint.value = ok ? '已複製' : '複製失敗，請再試一次'
  window.setTimeout(() => {
    copyLogHint.value = ''
  }, 2500)
}

function onDocumentVisibilityForSession() {
  if (document.visibilityState === 'visible') {
    void refreshBackgroundSession()
  }
}

onMounted(async () => {
  document.addEventListener('visibilitychange', onAppForegroundForPlayback)
  document.addEventListener('visibilitychange', onDocumentVisibilityForSession)
  document.addEventListener('click', onDocumentClickForPlayerMenu)
  reloadFavoritePlaylists()
  void refreshBackgroundSession()
  void loadVideoPlayerSettings()
  streamPrepUnlisten = await listenVideoStreamPrepProgress((ev) => {
    streamPrepProgress.value = ev
  })
})

onActivated(() => {
  void refreshBackgroundSession()
  window.setTimeout(() => {
    void refreshBackgroundSession()
  }, 300)
  if (!awaitingSystemPicker.value) {
    releaseBusyState()
    if (status.value === '正在開啟播放器…') {
      status.value = ''
    }
  }
  if (videoStreamSession.active && fullQueueCount.value > 0 && !videoStreamSession.playbackLocked) {
    void nextTick(() => processRemoteStreamQueue())
  }
})

watch(
  () => videoStreamSession.navigateSeq,
  () => {
    videoSubTab.value = 'current'
    void nextTick(() => processRemoteStreamQueue())
  },
)

onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', onAppForegroundForPlayback)
  document.removeEventListener('visibilitychange', onDocumentVisibilityForSession)
  document.removeEventListener('click', onDocumentClickForPlayerMenu)
  streamPrepUnlisten?.()
  streamPrepUnlisten = null
  endPickerWait()
  releaseBusyState()
  releaseStreamPlaybackLock()
})
</script>

<template>
  <div class="video-play-root">
    <Teleport to="body">
      <div
        v-if="awaitingSystemPicker"
        class="open-overlay open-overlay--picker"
        @click.stop
        @touchmove.stop.prevent
      >
        <div class="open-overlay-card">
          <p class="open-overlay-title">{{ status || '請在系統視窗選擇…' }}</p>
          <p class="open-overlay-sub">若已選完仍停在此畫面，請點取消後再試一次</p>
          <button type="button" class="reader-btn open-overlay-cancel" @click="cancelPickerWait()">
            取消
          </button>
        </div>
      </div>
      <div
        v-else-if="busy && pickMode === null"
        class="open-overlay open-overlay--picker"
        @click.stop
        @touchmove.stop.prevent
      >
        <div class="open-overlay-card">
          <p class="open-overlay-title">{{ status || '處理中…' }}</p>
          <button type="button" class="reader-btn open-overlay-cancel" @click="cancelPickerWait()">
            取消
          </button>
        </div>
      </div>
      <div
        v-else-if="streamPrepProgress"
        class="open-overlay open-overlay--stream-prep"
        @click.stop
        @touchmove.stop.prevent
      >
        <div class="open-overlay-card open-overlay-card--wide">
          <p class="open-overlay-title">串流播放準備中</p>
          <p v-if="streamPrepTitle" class="open-overlay-sub open-overlay-sub--wrap">{{ streamPrepTitle }}</p>
          <p class="open-overlay-sub">{{ streamPrepProgress.message }}</p>
          <p v-if="streamPrepProgress.bytesTotal > 0" class="stream-prep-detail">
            {{ formatSize(streamPrepProgress.bytesDone) }} / {{ formatSize(streamPrepProgress.bytesTotal) }}
            <span v-if="streamPrepProgress.speedBps > 0"> · {{ formatSpeed(streamPrepProgress.speedBps) }}</span>
          </p>
          <div
            v-if="streamPrepProgress.bytesTotal > 0"
            class="stream-prep-bar"
          >
            <div
              class="stream-prep-bar-fill"
              :style="{
                width: `${Math.min(100, (streamPrepProgress.bytesDone / streamPrepProgress.bytesTotal) * 100)}%`,
              }"
            />
          </div>
          <p
            v-else-if="streamPrepProgress.phase === 'checking' || streamPrepProgress.phase === 'launching'"
            class="stream-prep-detail stream-prep-detail--pulse"
          >
            請稍候…
          </p>
        </div>
      </div>
    </Teleport>

    <nav class="video-sub-nav" aria-label="影片播放子分頁">
      <button
        v-for="tab in videoSubTabs"
        :key="tab.key"
        type="button"
        class="video-sub-link"
        :class="{ on: videoSubTab === tab.key }"
        @click="setVideoSubTab(tab.key)"
      >
        <span class="video-sub-link-text">{{ tab.label }}</span>
        <span
          v-if="tab.key === 'current' && fullQueueCount > 0"
          class="video-sub-badge"
        >{{ fullQueueCount }}</span>
        <span
          v-if="tab.key === 'favorites' && favoritePlaylists.length > 0"
          class="video-sub-badge"
        >{{ favoritePlaylists.length }}</span>
        <span
          v-if="tab.key === 'log' && hasStreamLog"
          class="video-sub-dot"
          aria-hidden="true"
        />
      </button>
    </nav>

    <div class="video-sub-body">
    <!-- 播放主頁 -->
    <div v-show="videoSubTab === 'home'" class="video-sub-pane video-sub-pane--home">
      <p class="video-pane-lead">本地播放與遠端 PC 串流（外部播放器）</p>

      <div class="video-home-center">
      <div
        v-if="selectedPlayer"
        ref="playerDropdownRef"
        class="player-dropdown"
      >
        <button
          type="button"
          class="player-dropdown-trigger"
          :disabled="playerPickerBusy"
          :aria-expanded="playerMenuOpen"
          @click.stop="togglePlayerMenu"
        >
          <img
            v-if="selectedPlayer.iconDataUrl"
            :src="selectedPlayer.iconDataUrl"
            alt=""
            class="player-dropdown-icon"
          />
          <span v-else class="player-dropdown-icon player-dropdown-icon--fallback" aria-hidden="true">▶</span>
          <span class="player-dropdown-trigger-text">
            <span class="player-dropdown-trigger-label">預設播放器</span>
            <span class="player-dropdown-trigger-value">{{ selectedPlayer.name }}</span>
          </span>
          <span class="player-dropdown-chevron" :class="{ open: playerMenuOpen }" aria-hidden="true">▾</span>
        </button>

        <div v-show="playerMenuOpen" class="player-dropdown-menu">
          <div
            class="player-dropdown-current"
            :title="selectedPlayer.exePath ?? '系統預設關聯程式'"
          >
            <img
              v-if="selectedPlayer.iconDataUrl"
              :src="selectedPlayer.iconDataUrl"
              alt=""
              class="player-dropdown-icon"
            />
            <span v-else class="player-dropdown-icon player-dropdown-icon--fallback" aria-hidden="true">▶</span>
            <span class="player-dropdown-name">{{ selectedPlayer.name }}</span>
          </div>

          <template v-if="otherPlayerOptions.length > 0">
            <div class="player-dropdown-divider">其他選項</div>
            <div class="player-dropdown-list">
              <button
                v-for="player in otherPlayerOptions"
                :key="player.id"
                type="button"
                class="player-dropdown-item"
                :disabled="playerPickerBusy"
                :title="player.exePath ?? '系統預設'"
                @click="chooseVideoPlayer(player)"
              >
                <img
                  v-if="player.iconDataUrl"
                  :src="player.iconDataUrl"
                  alt=""
                  class="player-dropdown-icon"
                />
                <span v-else class="player-dropdown-icon player-dropdown-icon--fallback" aria-hidden="true">▶</span>
                <span class="player-dropdown-name">{{ player.name }}</span>
              </button>
            </div>
          </template>

          <button
            type="button"
            class="player-dropdown-browse"
            :disabled="playerPickerBusy"
            @click="browseVideoPlayerExe"
          >
            瀏覽其他程式…
          </button>
        </div>
      </div>
      </div>

      <div class="idle-actions idle-actions--home">
        <button
          type="button"
          class="reader-btn reader-btn--primary reader-btn--fit"
          :disabled="busy"
          @click="openLocalVideo"
        >
          開啟本機影片檔
        </button>
      </div>

      <div v-if="canShowResume" class="stream-resume-card">
        <p class="stream-resume-title">可接續觀看</p>
        <p class="stream-resume-name">{{ resumeTitle }}</p>
        <div class="stream-resume-actions">
          <button
            type="button"
            class="reader-btn reader-btn--primary"
            @click="canResumeBackground ? resumeBackgroundPlayback() : resumeCurrentStream()"
          >
            繼續播放
          </button>
          <button
            type="button"
            class="reader-btn"
            @click="canResumeBackground ? endBackgroundPlayback() : endCurrentStream()"
          >
            結束播放
          </button>
        </div>
      </div>

      <p class="folder-hint">
        遠端影片：在「遠端管理」勾選後用「串流 → 串流播放」。播放時會用上方選定的外部播放器開啟。
      </p>

      <p
        v-if="status && !awaitingSystemPicker && !busy"
        class="reader-ph status-line"
        :class="{ err: status.includes('失敗') || status.includes('逾時') || status.includes('錯誤') }"
      >
        {{ status }}
      </p>
    </div>

    <!-- 串流列表 -->
    <div v-show="videoSubTab === 'current'" class="video-sub-pane video-sub-pane--current">
      <div class="video-pane-toolbar">
        <p class="video-pane-heading">串流列表（{{ fullQueueCount }}）</p>
        <div class="video-pane-toolbar-actions">
          <button
            type="button"
            class="reader-btn"
            :disabled="fullQueueCount === 0"
            @click="onStarCurrentPlaylist"
          >
            ★ 收藏此列表
          </button>
          <button
            type="button"
            class="reader-btn"
            :disabled="fullQueueCount === 0"
            @click="onClearCurrentPlaylist"
          >
            清空列表
          </button>
        </div>
      </div>
      <div class="video-list-fill">
        <StreamPlaylistList
          fill
          :jobs="fullStreamQueue"
          :current-key="currentJobKey"
          :progress-for-job="videoProgressLabel"
          :progress-ratio-for-job="videoProgressRatioForJob"
          :opened-for-job="videoOpenedMark"
          empty-text="尚無項目；請於遠端管理勾選影片後「串流播放」"
          @play="onPickQueueJob"
        />
      </div>
    </div>

    <!-- 串流收藏 -->
    <div v-show="videoSubTab === 'favorites'" class="video-sub-pane video-sub-pane--favorites">
      <p class="video-pane-heading video-pane-heading--solo">串流收藏（{{ favoritePlaylists.length }}）</p>
      <div class="fav-pl-scroll">
        <p v-if="favoritePlaylists.length === 0" class="stream-pl-empty">
          尚無收藏；在「串流列表」按「★ 收藏此列表」
        </p>
        <div
          v-for="pl in favoritePlaylists"
          :key="pl.id"
          class="fav-pl-block"
          :class="{ 'fav-pl-block--open': isFavoriteExpanded(pl.id) }"
        >
          <div class="fav-pl-header" @click="toggleFavoriteExpanded(pl.id)">
            <span class="fav-pl-caret" aria-hidden="true">{{
              isFavoriteExpanded(pl.id) ? '▼' : '▶'
            }}</span>
            <input
              class="fav-pl-name"
              type="text"
              :value="pl.name"
              maxlength="48"
              @click.stop
              @change="
                favoritePlaylists = renameFavoriteStreamPlaylist(
                  pl.id,
                  ($event.target as HTMLInputElement).value,
                )
              "
              @blur="
                favoritePlaylists = renameFavoriteStreamPlaylist(
                  pl.id,
                  ($event.target as HTMLInputElement).value,
                )
              "
            />
            <div class="fav-pl-header-actions" @click.stop>
              <button
                type="button"
                class="reader-btn reader-btn--primary"
                @click="onPlayFavoritePlaylist(pl)"
              >
                播放
              </button>
              <button type="button" class="reader-btn" @click="onDeleteFavoritePlaylist(pl)">
                刪除
              </button>
            </div>
          </div>
          <div v-show="isFavoriteExpanded(pl.id)" class="fav-pl-body">
            <StreamPlaylistList
              fill
              :jobs="pl.jobs"
              :progress-for-job="videoProgressLabel"
              :progress-ratio-for-job="videoProgressRatioForJob"
              :opened-for-job="videoOpenedMark"
              show-remove
              @play="(job) => onPickFavoriteQueueJob(pl, job)"
              @remove="(job) => (favoritePlaylists = removeFavoriteStreamPlaylistItem(pl.id, job))"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- 串流日誌 -->
    <div v-show="videoSubTab === 'log'" class="video-sub-pane video-sub-pane--log">
      <div class="video-stream-log-wrap">
        <p class="video-stream-log-label">串流日誌</p>
        <textarea
          ref="streamLogRef"
          class="video-stream-log"
          readonly
          :value="streamLogText"
          placeholder="遠端串流播放後會顯示於此"
          aria-label="串流日誌"
        />
        <div class="video-stream-log-actions">
          <button type="button" class="reader-btn" :disabled="!hasStreamLog" @click="copyStreamLog">
            複製日誌
          </button>
          <span v-if="copyLogHint" class="video-stream-log-hint">{{ copyLogHint }}</span>
        </div>
        <p v-if="!hasStreamLog" class="video-log-empty-hint">尚無日誌記錄</p>
      </div>
    </div>
    </div>
  </div>
</template>

<style scoped>
.video-play-root {
  --vp-pad-y: clamp(6px, 1.6vw, 12px);
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  max-width: none;
  padding: 0;
  text-align: left;
}

.video-sub-nav {
  display: flex;
  flex-shrink: 0;
  align-items: stretch;
  gap: 0;
  width: 100%;
  padding: 2px 0 0;
  box-sizing: border-box;
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(0, 0, 0, 0.2);
}

.video-sub-link {
  flex: 1 1 0;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--gm-sub-tab-gap);
  padding: var(--gm-sub-tab-pad-y) var(--gm-sub-tab-pad-x);
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: #888;
  font-size: var(--gm-sub-tab-fs);
  line-height: 1.15;
}

.video-sub-link.on {
  color: #6eb5ff;
  border-bottom-color: #3d6ef5;
}

.video-sub-link-text {
  flex: 1 1 auto;
  min-width: 0;
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: center;
}

.video-sub-badge {
  flex-shrink: 0;
  min-width: 16px;
  padding: 0 clamp(3px, 1vw, 5px);
  border-radius: 9px;
  background: #3d6ef5;
  color: #fff;
  font-size: clamp(9px, 2.2vw, 10px);
  line-height: 16px;
}

.video-sub-dot {
  flex-shrink: 0;
  width: clamp(5px, 1.2vw, 6px);
  height: clamp(5px, 1.2vw, 6px);
  border-radius: 50%;
  background: #6eb5ff;
}

.video-sub-body {
  position: relative;
  flex: 1 1 auto;
  min-height: 0;
  width: 100%;
  overflow: hidden;
}

.video-sub-pane {
  position: absolute;
  inset: 0;
  box-sizing: border-box;
  width: 100%;
  overflow-x: hidden;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  padding-top: var(--vp-pad-y);
  padding-bottom: calc(var(--vp-pad-y) + env(safe-area-inset-bottom, 0px));
  padding-left: max(6px, env(safe-area-inset-left, 0px));
  padding-right: max(6px, env(safe-area-inset-right, 0px));
}

.video-sub-pane--log,
.video-sub-pane--current,
.video-sub-pane--favorites,
.video-sub-pane--home {
  display: flex;
  flex-direction: column;
}

.video-sub-pane--home {
  align-items: center;
  text-align: center;
}

.video-sub-pane--home .video-pane-lead,
.video-sub-pane--home .folder-hint,
.video-sub-pane--home .status-line {
  width: 100%;
  max-width: min(100%, 480px);
}

.video-sub-pane--home .idle-actions--home,
.video-sub-pane--home .stream-resume-card,
.video-sub-pane--home .video-home-center {
  width: 100%;
  max-width: min(100%, 420px);
}

.video-home-center {
  margin: 0 auto 16px;
  display: flex;
  justify-content: center;
}

.player-dropdown {
  position: relative;
  width: 100%;
  margin: 0;
  text-align: left;
}

.player-dropdown-trigger {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 10px 12px;
  box-sizing: border-box;
  border: 1px solid #555;
  border-radius: 6px;
  background: #2b2b2b;
  color: #eee;
  cursor: pointer;
  text-align: left;
}

.player-dropdown-trigger:hover:not(:disabled) {
  border-color: #6eb5ff;
  background: #333;
}

.player-dropdown-trigger:disabled {
  opacity: 0.65;
  cursor: wait;
}

.player-dropdown-trigger-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.player-dropdown-trigger-label {
  font-size: 11px;
  color: #999;
  line-height: 1.2;
}

.player-dropdown-trigger-value {
  font-size: 14px;
  font-weight: 600;
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.player-dropdown-chevron {
  flex-shrink: 0;
  font-size: 14px;
  color: #aaa;
  transition: transform 0.15s ease;
}

.player-dropdown-chevron.open {
  transform: rotate(180deg);
}

.player-dropdown-menu {
  position: absolute;
  z-index: 100;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  overflow: hidden;
  border: 1px solid #ccc;
  border-radius: 4px;
  background: #fff;
  color: #1a1a1a;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.player-dropdown-current {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  background: #0078d4;
  color: #fff;
}

.player-dropdown-divider {
  padding: 8px 12px 4px;
  font-size: 12px;
  color: #666;
  border-top: 1px solid #e5e5e5;
}

.player-dropdown-list {
  display: flex;
  flex-direction: column;
  max-height: 280px;
  overflow-y: auto;
}

.player-dropdown-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 8px 12px;
  box-sizing: border-box;
  border: none;
  background: #fff;
  color: #1a1a1a;
  text-align: left;
  cursor: pointer;
}

.player-dropdown-item:hover:not(:disabled) {
  background: #f3f3f3;
}

.player-dropdown-item:disabled {
  opacity: 0.6;
  cursor: wait;
}

.player-dropdown-browse {
  display: block;
  width: 100%;
  padding: 10px 12px;
  border: none;
  border-top: 1px solid #e5e5e5;
  background: #fafafa;
  color: #0078d4;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.player-dropdown-browse:hover:not(:disabled) {
  background: #f0f0f0;
}

.player-dropdown-browse:disabled {
  opacity: 0.6;
  cursor: wait;
}

.player-dropdown-icon {
  flex-shrink: 0;
  width: 32px;
  height: 32px;
  object-fit: contain;
  border-radius: 2px;
}

.player-dropdown-icon--fallback {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.08);
  color: #666;
  font-size: 14px;
}

.player-dropdown-current .player-dropdown-icon--fallback {
  background: rgba(255, 255, 255, 0.2);
  color: #fff;
}

.player-dropdown-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
  line-height: 1.35;
}

.video-sub-pane--current,
.video-sub-pane--favorites {
  overflow: hidden;
}

.video-pane-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-shrink: 0;
  margin-bottom: 8px;
}

.video-pane-toolbar-actions {
  display: flex;
  flex-shrink: 0;
  gap: 6px;
  margin-left: auto;
}

.video-pane-heading {
  margin: 0;
  font-size: clamp(12px, 3vw, 14px);
  font-weight: 600;
  line-height: 1.35;
  flex-shrink: 0;
}

.video-pane-heading--solo {
  margin: 0 0 8px;
}

.video-list-fill {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.video-list-fill :deep(.stream-pl-empty) {
  margin: 0;
}

.fav-pl-scroll {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
}

.fav-pl-block--open {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.video-pane-lead {
  margin: 0 0 12px;
  font-size: clamp(12px, 3.2vw, 14px);
  line-height: 1.4;
  opacity: 0.85;
}

.idle-actions {
  display: flex;
  flex-wrap: wrap;
  gap: clamp(6px, 2vw, 10px);
  width: 100%;
}

.idle-actions--home {
  justify-content: center;
}

.reader-btn--fit {
  flex: 0 0 auto;
  width: auto;
  min-width: 0;
  max-width: 100%;
  white-space: nowrap;
}

.idle-actions .reader-btn:not(.reader-btn--fit) {
  flex: 1 1 auto;
  min-width: min(100%, 200px);
  max-width: 100%;
}

.folder-hint {
  margin: 0;
  width: 100%;
  max-width: 100%;
  font-size: clamp(10px, 2.8vw, 12px);
  line-height: 1.45;
  color: #888;
  word-break: break-word;
  overflow-wrap: anywhere;
}

.video-stream-log-wrap {
  margin-top: 0;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.video-stream-log-label {
  margin: 0 0 6px;
  font-size: 12px;
  opacity: 0.85;
}

.video-stream-log {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 0;
  padding: clamp(6px, 2vw, 10px);
  flex: 1 1 auto;
  min-height: clamp(100px, 28dvh, 280px);
  max-height: none;
  overflow: auto;
  resize: none;
  font-family: ui-monospace, monospace;
  font-size: clamp(9px, 2.4vw, 11px);
  line-height: 1.35;
  white-space: pre-wrap;
  word-break: break-all;
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid #444;
  border-radius: 6px;
  color: #ddd;
}

.video-stream-log-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
}

.video-stream-log-hint {
  font-size: 11px;
  color: #8ab4f8;
}

.stream-resume-card {
  margin: 12px 0;
  padding: clamp(10px, 3vw, 14px);
  width: 100%;
  max-width: 100%;
  box-sizing: border-box;
  border: 1px solid #3a5a8a;
  border-radius: 8px;
  background: rgba(26, 39, 68, 0.45);
}

.stream-resume-title {
  margin: 0 0 6px;
  font-size: 13px;
  color: #8ab4f8;
}

.stream-resume-name {
  margin: 0 0 10px;
  font-size: 12px;
  line-height: 1.4;
  word-break: break-all;
  color: #eee;
}

.stream-resume-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.status-line {
  word-break: break-all;
  overflow-wrap: anywhere;
  line-height: 1.45;
}

.video-log-empty-hint {
  margin: 8px 0 0;
  font-size: 12px;
  opacity: 0.65;
}

.fav-pl-block {
  margin-bottom: 10px;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  overflow: hidden;
  background: rgba(0, 0, 0, 0.2);
}
.fav-pl-header {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: clamp(8px, 2.5vw, 10px);
  background: rgba(0, 0, 0, 0.25);
  cursor: pointer;
}
.fav-pl-caret {
  flex-shrink: 0;
  width: 1em;
  font-size: 11px;
  opacity: 0.85;
}
.fav-pl-name {
  flex: 1;
  min-width: 0;
  padding: 4px 8px;
  border-radius: 6px;
  border: 1px solid #444;
  background: rgba(0, 0, 0, 0.2);
  color: inherit;
  font-weight: 600;
  font-size: clamp(12px, 3vw, 14px);
}
.fav-pl-header-actions {
  display: flex;
  flex-shrink: 0;
  gap: 6px;
}
.fav-pl-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 6px 8px 8px;
}

.fav-pl-block:not(.fav-pl-block--open) {
  flex-shrink: 0;
}
</style>

<style>
.open-overlay {
  position: fixed;
  inset: 0;
  z-index: 200000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.72);
  padding: 24px;
}

.open-overlay--picker {
  background: rgba(0, 0, 0, 0.45);
}

.open-overlay-card {
  width: min(320px, 100%);
  padding: 20px 18px;
  border-radius: 10px;
  background: #1e1e1e;
  border: 1px solid #444;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
}

.open-overlay-card--wide {
  width: min(360px, calc(100vw - 32px));
}

.open-overlay-title {
  margin: 0 0 14px;
  font-size: 13px;
  color: #eee;
  text-align: center;
  line-height: 1.45;
}

.open-overlay-title--wrap {
  text-align: left;
  word-break: break-word;
  overflow-wrap: anywhere;
}

.open-overlay-sub {
  margin: 8px 0 0;
  font-size: 11px;
  color: #888;
  text-align: center;
  line-height: 1.4;
}

.open-overlay-sub--wrap {
  text-align: left;
  word-break: break-all;
  overflow-wrap: anywhere;
}

.open-overlay--stream-hint {
  background: rgba(0, 0, 0, 0.55);
}

.open-overlay--stream-prep {
  z-index: 12000;
}

.stream-prep-detail {
  margin: 10px 0 0;
  font-size: 13px;
  color: #ccc;
  line-height: 1.45;
  text-align: left;
}

.stream-prep-detail--pulse {
  animation: stream-prep-pulse 1.2s ease-in-out infinite;
}

@keyframes stream-prep-pulse {
  0%,
  100% {
    opacity: 0.55;
  }
  50% {
    opacity: 1;
  }
}

.stream-prep-bar {
  margin-top: 12px;
  height: 6px;
  border-radius: 3px;
  background: rgba(255, 255, 255, 0.12);
  overflow: hidden;
}

.stream-prep-bar-fill {
  height: 100%;
  border-radius: 3px;
  background: linear-gradient(90deg, #0078d4, #6eb5ff);
  transition: width 0.2s ease;
}

.open-overlay-cancel {
  display: block;
  width: 100%;
  margin-top: 14px;
}
</style>
