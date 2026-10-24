<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  cancelRemotePcTransfer,
  cancelRemotePcFileOp,
  listRemotePcDirectory,
  listRemotePcFiles,
  pickRemoteTransferDestination,
  pickRemoteUploadFile,
  pickRemoteUploadFiles,
  pickRemoteUploadFolder,
  planRemotePcUpload,
  remotePcFileOp,
  remotePcPasteWithProgress,
  remotePcCrossPasteWithProgress,
  transferRemotePcFiles,
  uploadRemotePcFiles,
  type RemotePcFileOpAction,
  type RemotePcBrowseResult,
  type RemotePcDirEntry,
  type RemotePcListItem,
  type RemotePcTransferSelection,
  type RemoteTransferProgressEvent,
  type RemoteUploadConflictPolicy,
} from '../api'
import { listen } from '@tauri-apps/api/event'
import { formatInvokeError } from '../invokeError'
import { copyTextFromTextarea } from '../copyText'
import { getRemotePcFavoriteDisplayName } from '../remotePcFavoritesStorage'
import { rememberRemotePcHosts } from '../remotePcHostRegistry'
import {
  activateComicStreamQueueJob,
  comicStreamSession,
  queueRemoteComicStreamPlay,
  requestNavigateToComicStream,
  comicStreamLog,
} from '../comicStreamStore'
import {
  queueRemoteStreamPlay,
  requestNavigateToVideoTab,
  streamLog,
  type RemoteStreamJob,
} from '../videoStreamStore'
import {
  gridColsForRemoteBrowseLayout,
  isRemoteBrowseGridLayout,
  loadSavedRemoteBrowseLayout,
  REMOTE_BROWSE_LAYOUT_OPTIONS,
  saveRemoteBrowseLayout,
  type RemoteBrowseLayout,
} from '../remoteBrowseLayout'
import { clearRemoteBrowseCoverIndexCache } from '../remoteBrowseCover'
import { clearThumbCacheForHost, isComicZipFileName, isVideoFileName } from '../remoteBrowseMedia'
import { migrateStreamFavoritesMountPaths } from '../migrateFavoriteMountPaths'
import {
  clearRemotePcClipboard,
  isSameRemotePc,
  loadRemotePcClipboard,
  saveRemotePcClipboard,
} from '../remotePcClipboardStorage'
import { comicFavoriteAdapter } from '../comicStreamPlaylistStorage'
import { videoFavoriteAdapter } from '../streamPlaylistStorage'
import type {
  FavoriteJobGroup,
  FavoriteStreamAdapter,
  FavoriteStreamStateBase,
} from '../streamFavoriteTypes'
import RemoteBrowseEntryThumb from './RemoteBrowseEntryThumb.vue'
import StreamFavoriteAddDialog from './StreamFavoriteAddDialog.vue'

const props = defineProps<{
  pc: RemotePcListItem
  /** true 才 Teleport 底欄至 bottom-dock（主頁＋遠端管理）；否則不渲染 */
  dockFootEnabled: boolean
}>()

const emit = defineEmits<{
  exit: []
}>()

const currentPath = ref('')
const browse = ref<RemotePcBrowseResult | null>(null)
const loading = ref(false)
const error = ref('')
/** path -> 勾選當下正在瀏覽的目錄（用於手機端寫入路徑） */
const selectedItems = ref<Map<string, string>>(new Map())
const remoteBusy = ref(false)
const transferMenuOpen = ref(false)
const streamMenuOpen = ref(false)
const opMenuOpen = ref(false)
const opMenuBtnRef = ref<HTMLElement | null>(null)
const transferMenuBtnRef = ref<HTMLElement | null>(null)
const opMenuFloatStyle = ref<Record<string, string>>({})
const transferMenuFloatStyle = ref<Record<string, string>>({})
const renameDialogOpen = ref(false)
const renameDialogInput = ref('')
const renameDialogPath = ref('')
const renameDialogIsDir = ref(false)
const renameInputRef = ref<HTMLInputElement | null>(null)
/** 複選重新命名佇列（當前之後尚待處理） */
const renameQueue = ref<string[]>([])
const renameQueueTotal = ref(0)
const renameQueueIndex = ref(0)
type UploadSourceItem = { uri: string; kind: 'file' | 'folder'; label: string }
const uploadPickerOpen = ref(false)
const uploadSources = ref<UploadSourceItem[]>([])
const uploadPickerBusy = ref(false)
const mkdirDialogOpen = ref(false)
const mkdirDialogInput = ref('')
const mkdirInputRef = ref<HTMLInputElement | null>(null)
const deleteDialogOpen = ref(false)
const deleteDialogPaths = ref<string[]>([])
const uploadConflictOpen = ref(false)
const uploadConflictList = ref<string[]>([])
let pendingUploadConflictResolve: ((policy: RemoteUploadConflictPolicy | null) => void) | null =
  null
const transferProgress = ref<RemoteTransferProgressEvent | null>(null)
const transferStartedAt = ref(0)
const transferElapsedTick = ref(0)
let transferElapsedTimer: number | null = null
const transferDebugLog = ref<string[]>([])
const transferLogRef = ref<HTMLTextAreaElement | null>(null)
const copyLogHint = ref('')
const flashHint = ref('')
let flashHintTimer: ReturnType<typeof window.setTimeout> | null = null
/** 檔名排序：true=升序、false=降序（資料夾仍置於檔案前） */
const nameSortAscending = ref(true)
const viewLayout = ref<RemoteBrowseLayout>(loadSavedRemoteBrowseLayout())
const layoutMenuOpen = ref(false)
const layoutDropdownRef = ref<HTMLElement | null>(null)
let unlistenTransfer: (() => void) | null = null
/** 跨 PC 貼上走傳輸取消旗標，非同機 server paste */
const pasteCrossPc = ref(false)
/** 避免同一筆診斷 LOG 被進度輪詢重複追加 */
let lastTransferDiagLog = ''
let lastTransferLogSeq = 0
let lastTransferStatusKey = ''
/** 長時間（數 TB）傳輸只保留最近日誌，避免 WebView 記憶體／CPU 爆量白屏 */
const TRANSFER_LOG_MAX_LINES = 500

function isVideoFile(name: string): boolean {
  return isVideoFileName(name)
}

function isComicZipFile(name: string): boolean {
  return isComicZipFileName(name)
}

const isGridView = computed(() => isRemoteBrowseGridLayout(viewLayout.value))

const gridCols = computed(() => gridColsForRemoteBrowseLayout(viewLayout.value))

function appendTransferLog(line: string) {
  const ts = new Date().toLocaleString('zh-TW', { hour12: false })
  const log = transferDebugLog.value
  log.push(`[${ts}] ${line}`)
  if (log.length > TRANSFER_LOG_MAX_LINES) {
    log.splice(0, log.length - TRANSFER_LOG_MAX_LINES)
  }
}

function resetTransferLog() {
  transferDebugLog.value = []
  lastTransferDiagLog = ''
  lastTransferLogSeq = 0
  lastTransferStatusKey = ''
}

/** PC 端 detailLog 為滾動尾端：有 logSeq 時以序號差取尾端新行；舊版伺服端則比對前綴 */
function appendPcDetailLogDelta(diagRaw: string, logSeq?: number | null) {
  const diag = diagRaw.replace(/\r\n/g, '\n').trimEnd()
  if (!diag) return
  if (typeof logSeq === 'number' && logSeq > 0) {
    if (logSeq <= lastTransferLogSeq) return
    const lines = diag.split('\n').filter((l) => l.trim())
    const take = Math.min(logSeq - lastTransferLogSeq, lines.length, TRANSFER_LOG_MAX_LINES)
    lastTransferLogSeq = logSeq
    for (const line of lines.slice(lines.length - take)) {
      appendTransferLog(line.trim())
    }
    return
  }
  if (diag === lastTransferDiagLog) return
  let added = diag
  if (lastTransferDiagLog && diag.startsWith(lastTransferDiagLog)) {
    added = diag.slice(lastTransferDiagLog.length).replace(/^\n+/, '')
  } else if (lastTransferDiagLog && diag.includes('\n')) {
    added = diag
  }
  lastTransferDiagLog = diag
  const addedLines = added.split('\n')
  for (const line of addedLines.slice(-TRANSFER_LOG_MAX_LINES)) {
    const t = line.trim()
    if (t) appendTransferLog(t)
  }
}

const transferErrorMessage = computed(() => {
  const p = transferProgress.value
  if (!p) return ''
  if (p.finished && p.message) return p.message
  if (p.error) return formatInvokeError(p.error)
  if (p.finished && p.phase === 'error') return formatInvokeError(p.message)
  return p.message
})

const transferOverlayTitle = computed(() => {
  const p = transferProgress.value
  if (!p) return ''
  const isPaste =
    p.phase === 'pasting' ||
    p.phase === 'preparing' ||
    p.phase === 'scanning' ||
    p.phase === 'cancelling' ||
    p.message.includes('貼上') ||
    p.message.includes('計算大小')
  if (!p.finished) {
    if (p.phase === 'scanning' || p.message.includes('計算大小')) return '正在計算大小'
    if (isPaste) return '貼上中'
    return p.phase === 'uploading' || p.message.includes('上傳') ? '上傳中' : '下載中'
  }
  if (p.phase === 'cancelled') {
    if (isPaste) return '貼上已取消'
    return p.message.includes('上傳') ? '上傳已取消' : '下載已取消'
  }
  if (p.phase === 'partial') {
    return p.message.includes('上傳') ? '上傳完成（部分失敗）' : '下載完成（部分失敗）'
  }
  if (p.error || p.phase === 'error') {
    if (isPaste) return '貼上失敗'
    return p.message.includes('上傳') ? '上傳失敗' : '下載失敗'
  }
  if (isPaste) return '貼上完成'
  return p.message.includes('上傳') ? '上傳完成' : '下載完成'
})

const transferElapsedText = computed(() => {
  void transferElapsedTick.value
  if (!transferStartedAt.value) return ''
  const sec = Math.max(0, Math.floor((Date.now() - transferStartedAt.value) / 1000))
  const m = Math.floor(sec / 60)
  const s = sec % 60
  return m > 0 ? `用時 ${m}:${String(s).padStart(2, '0')}` : `用時 ${sec} 秒`
})

function startOverlayElapsedTimer() {
  transferStartedAt.value = Date.now()
  transferElapsedTick.value = 0
  if (transferElapsedTimer != null) {
    window.clearInterval(transferElapsedTimer)
  }
  transferElapsedTimer = window.setInterval(() => {
    transferElapsedTick.value++
  }, 1000)
}

function stopOverlayElapsedTimer() {
  if (transferElapsedTimer != null) {
    window.clearInterval(transferElapsedTimer)
    transferElapsedTimer = null
  }
}

const transferDebugText = computed(() => {
  if (transferDebugLog.value.length) return transferDebugLog.value.join('\n')
  return transferProgress.value?.detailLog ?? ''
})

const transferLogCopyText = computed(() => {
  const header = transferErrorMessage.value.trim()
  const body = transferDebugText.value.trim()
  if (header && body) return `${header}\n\n${body}`
  return header || body
})

const showTransferLog = computed(() => {
  const p = transferProgress.value
  if (!p?.finished) return false
  return Boolean(transferLogCopyText.value.trim())
})

function showFlashHint(message: string, durationMs = 1800) {
  flashHint.value = message
  if (flashHintTimer != null) {
    window.clearTimeout(flashHintTimer)
  }
  flashHintTimer = window.setTimeout(() => {
    flashHint.value = ''
    flashHintTimer = null
  }, durationMs)
}

async function copyTransferLog() {
  const text = transferLogCopyText.value
  if (!text.trim()) return
  copyLogHint.value = ''
  const ok = await copyTextFromTextarea(text, transferLogRef.value)
  copyLogHint.value = ok ? '已複製到剪貼簿' : '複製失敗，請再試一次'
  window.setTimeout(() => {
    copyLogHint.value = ''
  }, 2500)
}

const host = computed(() => props.pc.connectedHost ?? props.pc.hosts[0] ?? '')

const displayPcName = computed(() =>
  host.value
    ? getRemotePcFavoriteDisplayName(host.value, props.pc.port, props.pc.name)
    : props.pc.name,
)

watch(
  () => [displayPcName.value, host.value, props.pc.hosts, props.pc.port] as const,
  ([name, h, hosts, port]) => {
    if (!name || !h) return
    rememberRemotePcHosts(name, [h, ...hosts], port)
  },
  { immediate: true },
)

const pathLabel = computed(() => {
  if (!browse.value?.path) {
    return '（根目錄）'
  }
  return browse.value.pathDisplay?.trim() || browse.value.path
})

const isBrowseRoot = computed(() => !(browse.value?.path ?? '').trim())

function entryLabel(entry: RemotePcDirEntry): string {
  return entry.displayName?.trim() || entry.name
}

function isLegacyShareRootListing(result: RemotePcBrowseResult | null): boolean {
  if (!result || result.path) return false
  return result.entries.some(
    (e) =>
      e.isDir &&
      !e.displayName?.trim() &&
      (e.name === '分享' || e.name.startsWith('分享@') || /^分享/.test(e.name)),
  )
}

const pcShareRootsOutdated = computed(() => {
  const api = browse.value?.remoteApi
  if (api != null && api < 6) return true
  return isLegacyShareRootListing(browse.value)
})

const sortToggleLabel = computed(() => (nameSortAscending.value ? '升序' : '降序'))

const sortedEntries = computed(() => {
  const entries = browse.value?.entries ?? []
  const dir = nameSortAscending.value ? 1 : -1
  return [...entries].sort((a, b) => {
    if (a.isDir !== b.isDir) return a.isDir ? -1 : 1
    // 根目錄分享用 displayName（H:\…）；name 可能是 vol_<guid>，不可拿來排
    const cmp = entryLabel(a).localeCompare(entryLabel(b), 'zh-Hant', {
      numeric: true,
      sensitivity: 'base',
    })
    return cmp * dir
  })
})

function toggleNameSort() {
  nameSortAscending.value = !nameSortAscending.value
}

function toggleLayoutMenu() {
  layoutMenuOpen.value = !layoutMenuOpen.value
}

function closeLayoutMenu() {
  layoutMenuOpen.value = false
}

function placeMenuBelow(
  el: HTMLElement | null,
  styleRef: { value: Record<string, string> },
  align: 'left' | 'right',
) {
  if (!el) return
  const r = el.getBoundingClientRect()
  const top = `${Math.round(r.bottom + 6)}px`
  if (align === 'right') {
    styleRef.value = {
      position: 'fixed',
      top,
      left: 'auto',
      right: `${Math.max(8, Math.round(window.innerWidth - r.right))}px`,
      zIndex: '10000',
    }
  } else {
    styleRef.value = {
      position: 'fixed',
      top,
      left: `${Math.max(8, Math.round(r.left))}px`,
      right: 'auto',
      zIndex: '10000',
    }
  }
}

function toggleOpMenu() {
  const next = !opMenuOpen.value
  closeActionMenus()
  opMenuOpen.value = next
  if (next) {
    void nextTick(() => placeMenuBelow(opMenuBtnRef.value, opMenuFloatStyle, 'left'))
  }
}

function toggleTransferMenu() {
  const next = !transferMenuOpen.value
  closeActionMenus()
  transferMenuOpen.value = next
  if (next) {
    void nextTick(() => placeMenuBelow(transferMenuBtnRef.value, transferMenuFloatStyle, 'right'))
  }
}

function onDocumentClickForActionMenus() {
  if (opMenuOpen.value || transferMenuOpen.value) {
    closeActionMenus()
  }
}

function chooseBrowseLayout(layout: RemoteBrowseLayout) {
  viewLayout.value = layout
  saveRemoteBrowseLayout(layout)
  closeLayoutMenu()
}

function onDocumentClickForLayoutMenu(e: MouseEvent) {
  if (!layoutMenuOpen.value) return
  const el = layoutDropdownRef.value
  if (el && !el.contains(e.target as Node)) {
    closeLayoutMenu()
  }
}

const selectedCount = computed(() => selectedItems.value.size)

const allCurrentSelected = computed(() => {
  const entries = sortedEntries.value
  if (entries.length === 0) return false
  return entries.every((e) => selectedItems.value.has(entryPath(e)))
})

function formatSize(size: number | null): string {
  if (size === null) return ''
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

/** 即時速度；若 PC 回報 0 則用已傳位元組／用時估算 */
const transferDisplaySpeedBps = computed(() => {
  void transferElapsedTick.value
  const p = transferProgress.value
  if (!p || p.finished || p.bytesDone <= 0) return 0
  if (p.speedBps > 0) return p.speedBps
  if (!transferStartedAt.value) return 0
  const sec = Math.max(1, (Date.now() - transferStartedAt.value) / 1000)
  return Math.floor(p.bytesDone / sec)
})

/** 根目錄分享槽是否顯示磁碟容量（含進度條） */
function entryHasDiskCapacity(entry: RemotePcDirEntry): boolean {
  return (
    isBrowseRoot.value &&
    entry.isDir &&
    entry.diskFreeBytes != null &&
    entry.diskTotalBytes != null &&
    entry.diskTotalBytes > 0
  )
}

function entryDiskCapacityLabel(entry: RemotePcDirEntry): string {
  if (!entryHasDiskCapacity(entry)) return ''
  return `剩餘 ${formatSize(entry.diskFreeBytes!)}，共 ${formatSize(entry.diskTotalBytes!)}`
}

function entryDiskUsedPercent(entry: RemotePcDirEntry): number {
  if (!entryHasDiskCapacity(entry)) return 0
  const total = entry.diskTotalBytes!
  const free = entry.diskFreeBytes!
  const used = Math.max(0, total - free)
  return Math.min(100, Math.round((used / total) * 100))
}

/** 列表右側：檔案/資料夾大小（根目錄磁碟容量另用 entryDiskCapacityLabel） */
function entrySecondaryLabel(entry: RemotePcDirEntry): string {
  if (entryHasDiskCapacity(entry)) {
    return ''
  }
  if (entry.size != null) {
    return formatSize(entry.size)
  }
  return ''
}

function joinPath(base: string, name: string): string {
  if (!base) return name
  return `${base.replace(/\/+$/, '')}/${name}`
}

function parentPath(path: string): string {
  const idx = path.replace(/\\/g, '/').lastIndexOf('/')
  if (idx <= 0) return ''
  return path.slice(0, idx)
}

function baseName(path: string): string {
  const norm = path.replace(/\\/g, '/')
  const idx = norm.lastIndexOf('/')
  return idx >= 0 ? norm.slice(idx + 1) : norm
}

/** 原檔名副檔名（含 `.`）；資料夾或無副檔名則回傳空字串 */
function fileExtension(name: string): string {
  const idx = name.lastIndexOf('.')
  if (idx <= 0) return ''
  const ext = name.slice(idx)
  return ext.length >= 2 ? ext : ''
}

/** 使用者輸入是否含明確副檔名（如 `123.rar`） */
function userHasExplicitExtension(input: string): boolean {
  const idx = input.lastIndexOf('.')
  if (idx <= 0) return false
  return input.slice(idx + 1).trim().length > 0
}

/** 未指定副檔名時保留原名副檔名 */
function resolveRenameFinalName(originalName: string, userInput: string, isDir: boolean): string {
  const trimmed = userInput.trim()
  if (!trimmed) return ''
  if (isDir) return trimmed
  const origExt = fileExtension(originalName)
  if (!origExt) return trimmed
  if (userHasExplicitExtension(trimmed)) return trimmed
  const base = trimmed.replace(/\.+$/, '')
  return `${base}${origExt}`
}

function entryPath(entry: RemotePcDirEntry): string {
  return joinPath(currentPath.value, entry.name)
}

function isSelected(entry: RemotePcDirEntry): boolean {
  return selectedItems.value.has(entryPath(entry))
}

function toggleSelect(entry: RemotePcDirEntry) {
  const path = entryPath(entry)
  const next = new Map(selectedItems.value)
  if (next.has(path)) {
    next.delete(path)
  } else {
    next.set(path, currentPath.value)
  }
  selectedItems.value = next
}

function toggleSelectAll() {
  const entries = sortedEntries.value
  if (entries.length === 0) return
  const next = new Map(selectedItems.value)
  if (allCurrentSelected.value) {
    for (const entry of entries) {
      next.delete(entryPath(entry))
    }
  } else {
    for (const entry of entries) {
      next.set(entryPath(entry), currentPath.value)
    }
  }
  selectedItems.value = next
}

async function loadDirectory(path: string) {
  if (!host.value) {
    error.value = '缺少 PC 連線位址'
    return
  }
  const normPath = (p: string) => p.replace(/\\/g, '/').replace(/\/+$/, '')
  const pathChanging = normPath(path) !== normPath(currentPath.value)
  if (pathChanging) {
    selectedItems.value = new Map()
  }
  loading.value = true
  error.value = ''
  try {
    // 僅同路徑重載（刪改後刷新）才清縮圖；進子目錄／返回時保留，避免瞬間打爆 PC 連線造成隨機缺圖
    if (!pathChanging) {
      clearThumbCacheForHost(host.value, props.pc.port)
      clearRemoteBrowseCoverIndexCache(host.value, props.pc.port)
    }
    browse.value = await listRemotePcDirectory(host.value, props.pc.port, path)
    currentPath.value = browse.value.path
    const mounts = browse.value.shareMounts ?? []
    if (mounts.length > 0) {
      migrateStreamFavoritesMountPaths(
        mounts.map((m) => ({
          label: m.label,
          display: m.display,
          volumeGuid: m.volumeGuid,
          legacyLabels: m.legacyLabels,
        })),
      )
    }
  } catch (e) {
    error.value = formatInvokeError(e)
    browse.value = null
  } finally {
    loading.value = false
  }
}

function openEntry(entry: RemotePcDirEntry) {
  if (!entry.isDir) return
  if (remoteBusy.value) return
  void loadDirectory(joinPath(currentPath.value, entry.name))
}

function onMainEntryClick(entry: RemotePcDirEntry) {
  if (entry.isDir) {
    openEntry(entry)
  }
}

function closeActionMenus() {
  transferMenuOpen.value = false
  streamMenuOpen.value = false
  opMenuOpen.value = false
  closeLayoutMenu()
}

type StreamJob = {
  host: string
  port: number
  relPath: string
  title: string
  pcName?: string
}

const favAddDialogOpen = ref(false)
const favAddDialogJobs = ref<RemoteStreamJob[]>([])
const favAddDialogGroups = ref<FavoriteJobGroup[]>([])
const favAddDialogAdapter = ref<FavoriteStreamAdapter<FavoriteStreamStateBase, RemoteStreamJob>>(
  videoFavoriteAdapter,
)
const favAddDialogFallback = ref('收藏')

function sortStreamJobs(jobs: StreamJob[]): StreamJob[] {
  return [...jobs].sort((a, b) =>
    a.relPath.localeCompare(b.relPath, undefined, { numeric: true, sensitivity: 'base' }),
  )
}

/** 每個勾選根路徑各自一組（收藏「新增名稱」逐組命名用） */
async function collectStreamJobGroupsFromSelection(
  matchFile: (name: string) => boolean,
): Promise<FavoriteJobGroup[]> {
  const h = host.value
  if (!h) return []
  const paths = Array.from(selectedItems.value.keys())
  if (paths.length === 0) return []
  const pcName = displayPcName.value || props.pc.name

  const lists = await Promise.all(
    paths.map((path) => listRemotePcFiles(h, props.pc.port, path)),
  )

  const groups: FavoriteJobGroup[] = []
  for (let i = 0; i < paths.length; i++) {
    const path = paths[i]!
    const files = lists[i] ?? []
    const seen = new Set<string>()
    const jobs: StreamJob[] = []
    for (const file of files) {
      const relPath = file.relativePath.replace(/\\/g, '/')
      const name = baseName(relPath)
      if (!matchFile(name) || seen.has(relPath)) continue
      seen.add(relPath)
      jobs.push({
        host: h,
        port: props.pc.port,
        relPath,
        title: name,
        pcName,
      })
    }
    if (jobs.length === 0) continue
    const folderLabel = baseName(path.replace(/\\/g, '/').replace(/\/+$/, '')) || path || '收藏'
    groups.push({ defaultName: folderLabel, jobs: sortStreamJobs(jobs) })
  }
  return groups
}

async function collectStreamJobsFromSelection(
  matchFile: (name: string) => boolean,
): Promise<StreamJob[]> {
  const groups = await collectStreamJobGroupsFromSelection(matchFile)
  const seen = new Set<string>()
  const jobs: StreamJob[] = []
  for (const g of groups) {
    for (const job of g.jobs) {
      if (seen.has(job.relPath)) continue
      seen.add(job.relPath)
      jobs.push(job)
    }
  }
  return sortStreamJobs(jobs)
}

function isStreamableMediaName(name: string): boolean {
  return isVideoFile(name) || isComicZipFile(name)
}

/** 一次掃描，自動拆成影片／漫畫佇列 */
async function collectPartitionedStreamJobs(): Promise<{
  videos: StreamJob[]
  comics: StreamJob[]
}> {
  const mixed = await collectStreamJobsFromSelection(isStreamableMediaName)
  const videos: StreamJob[] = []
  const comics: StreamJob[] = []
  for (const job of mixed) {
    const name = job.title || job.relPath
    if (isVideoFile(name)) videos.push(job)
    else if (isComicZipFile(name)) comics.push(job)
  }
  return { videos, comics }
}

/** 整合「串流閱讀／串流播放」：依副檔名自動辨識並加入對應佇列 */
async function startStreamAuto() {
  closeActionMenus()
  const h = host.value
  if (!h) {
    showFlashHint('未連線 PC')
    return
  }
  if (selectedItems.value.size === 0) {
    showFlashHint('請勾選影片、ZIP/CBZ 漫畫或資料夾')
    return
  }
  remoteBusy.value = true
  try {
    showFlashHint('正在掃描可串流檔案…', 2500)
    const { videos, comics } = await collectPartitionedStreamJobs()
    if (videos.length === 0 && comics.length === 0) {
      showFlashHint('所選項目內沒有可串流的影片或漫畫')
      return
    }
    if (videos.length > 0) {
      queueRemoteStreamPlay(videos)
      streamLog(`來源 PC：${h}:${props.pc.port}（${videos.length} 部影片）`)
    }
    if (comics.length > 0) {
      queueRemoteComicStreamPlay(comics)
      comicStreamLog(`來源 PC：${h}:${props.pc.port}（${comics.length} 本漫畫）`)
    }
    if (videos.length > 0 && comics.length > 0) {
      showFlashHint(`已加入 ${videos.length} 部影片、${comics.length} 本漫畫`, 2800)
      if (comics.length > videos.length) {
        if (comics.length === 1) activateComicStreamQueueJob(comics[0]!, comics)
        else requestNavigateToComicStream()
      } else {
        requestNavigateToVideoTab()
      }
      return
    }
    if (videos.length > 0) {
      requestNavigateToVideoTab()
      return
    }
    if (comics.length === 1) {
      activateComicStreamQueueJob(comics[0]!, comics)
    } else {
      requestNavigateToComicStream()
    }
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    remoteBusy.value = false
  }
}

async function startFavoriteComic() {
  closeActionMenus()
  const h = host.value
  if (!h) {
    showFlashHint('未連線 PC')
    return
  }
  if (selectedItems.value.size === 0) {
    showFlashHint('請勾選 ZIP/CBZ 漫畫或資料夾')
    return
  }
  remoteBusy.value = true
  try {
    showFlashHint('正在掃描漫畫檔…', 2500)
    const groups = await collectStreamJobGroupsFromSelection(isComicZipFile)
    if (groups.length === 0) {
      showFlashHint('所選項目內沒有可收藏的 ZIP/CBZ')
      return
    }
    favAddDialogAdapter.value = comicFavoriteAdapter
    favAddDialogFallback.value = '漫畫收藏'
    favAddDialogGroups.value = groups
    favAddDialogJobs.value = groups.flatMap((g) => g.jobs)
    favAddDialogOpen.value = true
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    remoteBusy.value = false
  }
}

async function startFavoriteVideo() {
  closeActionMenus()
  const h = host.value
  if (!h) {
    showFlashHint('未連線 PC')
    return
  }
  if (selectedItems.value.size === 0) {
    showFlashHint('請勾選影片檔或資料夾')
    return
  }
  remoteBusy.value = true
  try {
    showFlashHint('正在掃描影片…', 2500)
    const groups = await collectStreamJobGroupsFromSelection(isVideoFile)
    if (groups.length === 0) {
      showFlashHint('所選項目內沒有可收藏的影片')
      return
    }
    favAddDialogAdapter.value = videoFavoriteAdapter
    favAddDialogFallback.value = '播放列表'
    favAddDialogGroups.value = groups
    favAddDialogJobs.value = groups.flatMap((g) => g.jobs)
    favAddDialogOpen.value = true
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    remoteBusy.value = false
  }
}

function onFavoriteAddDialogDone() {
  showFlashHint('已更新收藏')
}

function goUp() {
  if (remoteBusy.value) return
  void loadDirectory(parentPath(currentPath.value))
}

function askUploadConflict(conflicts: string[]): Promise<RemoteUploadConflictPolicy | null> {
  uploadConflictList.value = conflicts
  uploadConflictOpen.value = true
  return new Promise((resolve) => {
    pendingUploadConflictResolve = resolve
  })
}

function resolveUploadConflict(policy: RemoteUploadConflictPolicy) {
  uploadConflictOpen.value = false
  pendingUploadConflictResolve?.(policy)
  pendingUploadConflictResolve = null
}

function cancelUploadConflict() {
  uploadConflictOpen.value = false
  pendingUploadConflictResolve?.(null)
  pendingUploadConflictResolve = null
}

function sourceLabelFromUri(uri: string, kind: 'file' | 'folder'): string {
  const norm = uri.replace(/\\/g, '/')
  const base = norm.split('/').filter(Boolean).pop() || uri
  try {
    return decodeURIComponent(base)
  } catch {
    return base
  }
}

function openUploadPicker() {
  transferMenuOpen.value = false
  streamMenuOpen.value = false
  opMenuOpen.value = false
  if (!host.value || remoteBusy.value) return
  uploadSources.value = []
  uploadPickerOpen.value = true
}

function closeUploadPicker() {
  if (uploadPickerBusy.value) return
  uploadPickerOpen.value = false
  uploadSources.value = []
}

function removeUploadSource(idx: number) {
  uploadSources.value = uploadSources.value.filter((_, i) => i !== idx)
}

async function addUploadFiles() {
  if (uploadPickerBusy.value) return
  uploadPickerBusy.value = true
  try {
    const uris = await pickRemoteUploadFiles()
    if (!uris?.length) return
    const existing = new Set(uploadSources.value.map((s) => s.uri))
    const next = [...uploadSources.value]
    for (const uri of uris) {
      if (!uri || existing.has(uri)) continue
      existing.add(uri)
      next.push({ uri, kind: 'file', label: sourceLabelFromUri(uri, 'file') })
    }
    uploadSources.value = next
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    uploadPickerBusy.value = false
  }
}

async function addUploadFolder() {
  if (uploadPickerBusy.value) return
  uploadPickerBusy.value = true
  try {
    const uri = await pickRemoteUploadFolder()
    if (!uri) return
    if (uploadSources.value.some((s) => s.uri === uri)) {
      showFlashHint('此資料夾已在清單中')
      return
    }
    uploadSources.value = [
      ...uploadSources.value,
      { uri, kind: 'folder', label: sourceLabelFromUri(uri, 'folder') },
    ]
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    uploadPickerBusy.value = false
  }
}

async function confirmUploadPicker() {
  if (!host.value || remoteBusy.value || uploadPickerBusy.value) return
  if (uploadSources.value.length === 0) {
    showFlashHint('請先加入檔案或資料夾')
    return
  }
  const sources = [...uploadSources.value]
  uploadPickerOpen.value = false
  uploadSources.value = []
  try {
    const allFiles: Awaited<ReturnType<typeof planRemotePcUpload>>['files'] = []
    const conflictSet = new Set<string>()
    for (const src of sources) {
      const plan = await planRemotePcUpload(
        host.value,
        props.pc.port,
        currentPath.value,
        src.uri,
        src.kind,
      )
      allFiles.push(...plan.files)
      for (const c of plan.conflicts) conflictSet.add(c)
    }
    if (allFiles.length === 0) {
      showFlashHint('沒有可上傳的檔案')
      return
    }
    let policy: RemoteUploadConflictPolicy = 'overwrite'
    const conflicts = [...conflictSet]
    if (conflicts.length > 0) {
      const chosen = await askUploadConflict(conflicts)
      if (!chosen) return
      policy = chosen
    }
    remoteBusy.value = true
    resetTransferLog()
    appendTransferLog(
      `開始上傳 ${sources.length} 個來源 → PC ${host.value}:${props.pc.port}（${pathLabel.value}）`,
    )
    startOverlayElapsedTimer()
    transferProgress.value = {
      phase: 'uploading',
      fileIndex: 0,
      fileCount: 0,
      bytesDone: 0,
      bytesTotal: 0,
      speedBps: 0,
      message: '準備上傳…',
      finished: false,
      error: null,
    }
    await uploadRemotePcFiles(host.value, props.pc.port, allFiles, policy)
    void loadDirectory(currentPath.value)
  } catch (e) {
    const errText = formatInvokeError(e)
    appendTransferLog(`上傳 invoke 失敗：${errText}`)
    console.error('[remote-upload]', e)
    if (!transferProgress.value?.finished) {
      transferProgress.value = {
        phase: 'error',
        fileIndex: transferProgress.value?.fileIndex ?? 0,
        fileCount: transferProgress.value?.fileCount ?? 0,
        bytesDone: transferProgress.value?.bytesDone ?? 0,
        bytesTotal: transferProgress.value?.bytesTotal ?? 0,
        speedBps: 0,
        message: errText,
        finished: true,
        error: errText,
        detailLog: [...transferDebugLog.value, errText].join('\n'),
      }
      stopOverlayElapsedTimer()
    }
  } finally {
    remoteBusy.value = false
  }
}

async function startDownload() {
  transferMenuOpen.value = false
  streamMenuOpen.value = false
  opMenuOpen.value = false
  if (!host.value || remoteBusy.value) return
  if (selectedCount.value === 0) {
    showFlashHint('請先勾選要下載的檔案或資料夾')
    return
  }
  const dest = await pickRemoteTransferDestination()
  if (!dest) return
  remoteBusy.value = true
  resetTransferLog()
  appendTransferLog(`開始下載 → PC ${host.value}:${props.pc.port}`)
  startOverlayElapsedTimer()
  transferProgress.value = {
    phase: 'collecting',
    fileIndex: 0,
    fileCount: 0,
    bytesDone: 0,
    bytesTotal: 0,
    speedBps: 0,
    message: '準備下載…',
    finished: false,
    error: null,
  }
  try {
    const selections: RemotePcTransferSelection[] = Array.from(
      selectedItems.value.entries(),
    ).map(([path, anchorPath]) => ({ path, anchorPath }))
    for (const sel of selections) {
      appendTransferLog(`勾選 path=${sel.path} anchor=${sel.anchorPath}`)
    }
    appendTransferLog(`手機目標 URI 長度 ${dest.length}`)
    await transferRemotePcFiles(host.value, props.pc.port, selections, dest)
    selectedItems.value = new Map()
  } catch (e) {
    const errText = formatInvokeError(e)
    appendTransferLog(`invoke 失敗：${errText}`)
    console.error('[remote-transfer]', e)
    if (!transferProgress.value?.finished) {
      transferProgress.value = {
        phase: 'error',
        fileIndex: transferProgress.value?.fileIndex ?? 0,
        fileCount: transferProgress.value?.fileCount ?? 0,
        bytesDone: transferProgress.value?.bytesDone ?? 0,
        bytesTotal: transferProgress.value?.bytesTotal ?? 0,
        speedBps: 0,
        message: errText,
        finished: true,
        error: errText,
        detailLog: [...transferDebugLog.value, errText].join('\n'),
      }
    }
  } finally {
    remoteBusy.value = false
  }
}

function closeTransferOverlay() {
  transferProgress.value = null
  resetTransferLog()
  copyLogHint.value = ''
  stopOverlayElapsedTimer()
  transferStartedAt.value = 0
}

async function cancelActiveTransfer() {
  if (!transferProgress.value || transferProgress.value.finished) return
  const p = transferProgress.value
  const isPaste =
    p.phase === 'pasting' || p.phase === 'preparing' || p.phase === 'scanning' || p.phase === 'cancelling'
  appendTransferLog(isPaste ? '使用者要求取消貼上…' : '使用者要求取消傳輸…')
  transferProgress.value = {
    ...p,
    phase: 'cancelling',
    message: isPaste ? '正在取消貼上…' : '正在取消傳輸…',
    speedBps: 0,
  }
  try {
    if (isPaste) {
      await cancelRemotePcFileOp()
    } else {
      // 上傳若走「目的端拉取」，需同時取消目的 PC 的 file-op job
      await Promise.allSettled([cancelRemotePcTransfer(), cancelRemotePcFileOp()])
    }
  } catch (e) {
    appendTransferLog(`取消指令失敗：${formatInvokeError(e)}`)
  }
}

async function startPaste() {
  if (!host.value || remoteBusy.value) return
  remoteBusy.value = true
  resetTransferLog()
  const clip = loadRemotePcClipboard()
  const crossPc = clip != null && !isSameRemotePc(clip, host.value, props.pc.port)
  pasteCrossPc.value = crossPc
  appendTransferLog(
    crossPc && clip
      ? `開始跨 PC 貼上：${clip.sourceHost}:${clip.sourcePort} → ${host.value}:${props.pc.port}（${pathLabel.value}）`
      : `開始貼上 → PC ${host.value}:${props.pc.port}（${pathLabel.value}）`,
  )
  startOverlayElapsedTimer()
  transferProgress.value = {
    phase: 'preparing',
    fileIndex: 0,
    fileCount: 0,
    bytesDone: 0,
    bytesTotal: 0,
    speedBps: 0,
    message: '準備貼上…',
    finished: false,
    error: null,
  }
  try {
    if (crossPc && clip) {
      await remotePcCrossPasteWithProgress(
        clip.sourceHost,
        clip.sourcePort,
        clip.entries,
        host.value,
        props.pc.port,
        currentPath.value,
        clip.mode,
      )
      if (clip.mode === 'cut') clearRemotePcClipboard()
    } else {
      await remotePcPasteWithProgress(host.value, props.pc.port, currentPath.value)
      if (clip?.mode === 'cut' && isSameRemotePc(clip, host.value, props.pc.port)) {
        clearRemotePcClipboard()
      }
    }
    await loadDirectory(currentPath.value)
  } catch (e) {
    const errText = formatInvokeError(e)
    appendTransferLog(`貼上 invoke 失敗：${errText}`)
    console.error('[remote-paste]', e)
    if (!transferProgress.value?.finished) {
      transferProgress.value = {
        phase: 'error',
        fileIndex: transferProgress.value?.fileIndex ?? 0,
        fileCount: transferProgress.value?.fileCount ?? 0,
        bytesDone: transferProgress.value?.bytesDone ?? 0,
        bytesTotal: transferProgress.value?.bytesTotal ?? 0,
        speedBps: 0,
        message: errText,
        finished: true,
        error: errText,
      }
    }
  } finally {
    pasteCrossPc.value = false
    remoteBusy.value = false
  }
}

async function runFileOp(action: RemotePcFileOpAction) {
  opMenuOpen.value = false
  transferMenuOpen.value = false
  streamMenuOpen.value = false
  if (!host.value || remoteBusy.value) return
  const paths = Array.from(selectedItems.value.keys())

  if (action === 'rename') {
    if (paths.length === 0) {
      showFlashHint('請先勾選要重新命名的項目')
      return
    }
    startRenameQueue(paths)
    return
  }

  if (action === 'mkdir') {
    openMkdirDialog()
    return
  }

  if (action === 'delete') {
    if (paths.length === 0) {
      showFlashHint('請先勾選要刪除的項目')
      return
    }
    deleteDialogPaths.value = paths
    deleteDialogOpen.value = true
    return
  }

  if (action === 'paste') {
    await startPaste()
    return
  }

  remoteBusy.value = true
  try {
    if (paths.length === 0) {
      showFlashHint('請先勾選項目')
      return
    }

    const result = await remotePcFileOp(
      host.value,
      props.pc.port,
      action,
      paths,
      currentPath.value,
      '',
    )
    if (action === 'copy' || action === 'cut') {
      saveRemotePcClipboard({
        mode: action,
        sourceHost: host.value,
        sourcePort: props.pc.port,
        entries: paths.map((path) => ({
          path,
          anchorPath: selectedItems.value.get(path) ?? currentPath.value,
        })),
        savedAt: Date.now(),
      })
    }
    if (action === 'cut' || action === 'delete_recycle' || action === 'delete_permanent') {
      selectedItems.value = new Map()
    }
    await loadDirectory(currentPath.value)
    showFlashHint(result.message)
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    remoteBusy.value = false
  }
}

function openRenameDialog(relPath: string) {
  const oldName = baseName(relPath)
  const entry = sortedEntries.value.find((e) => entryPath(e) === relPath)
  renameDialogPath.value = relPath
  renameDialogIsDir.value = entry?.isDir ?? false
  renameDialogInput.value = oldName
  renameDialogOpen.value = true
  void nextTick(() => {
    const el = renameInputRef.value
    if (!el) return
    el.focus()
    el.select()
  })
}

function startRenameQueue(paths: string[]) {
  renameQueue.value = [...paths]
  renameQueueTotal.value = paths.length
  renameQueueIndex.value = 0
  const first = renameQueue.value.shift()
  if (!first) return
  renameQueueIndex.value = 1
  openRenameDialog(first)
}

function advanceRenameQueue() {
  const next = renameQueue.value.shift()
  if (!next) {
    renameDialogOpen.value = false
    renameDialogPath.value = ''
    renameQueueTotal.value = 0
    renameQueueIndex.value = 0
    selectedItems.value = new Map()
    void loadDirectory(currentPath.value)
    return
  }
  renameQueueIndex.value += 1
  openRenameDialog(next)
}

function cancelRenameDialog() {
  // 取消目前這一個，後續繼續
  renameDialogOpen.value = false
  renameDialogPath.value = ''
  advanceRenameQueue()
}

function abortAllRenameDialog() {
  renameQueue.value = []
  renameDialogOpen.value = false
  renameDialogPath.value = ''
  renameQueueTotal.value = 0
  renameQueueIndex.value = 0
  showFlashHint('已全部終止命名')
}

function openMkdirDialog() {
  mkdirDialogInput.value = ''
  mkdirDialogOpen.value = true
  void nextTick(() => {
    const el = mkdirInputRef.value
    if (!el) return
    el.focus()
  })
}

function cancelMkdirDialog() {
  mkdirDialogOpen.value = false
  mkdirDialogInput.value = ''
}

async function confirmMkdirDialog() {
  if (!host.value || remoteBusy.value) return
  const name = mkdirDialogInput.value.trim()
  if (!name || name === '.' || name === '..') {
    showFlashHint('資料夾名稱不可為空')
    return
  }
  if (/[\\/]/.test(name)) {
    showFlashHint('名稱不可含 / 或 \\')
    return
  }
  mkdirDialogOpen.value = false
  remoteBusy.value = true
  try {
    const result = await remotePcFileOp(
      host.value,
      props.pc.port,
      'mkdir',
      [],
      currentPath.value,
      name,
    )
    await loadDirectory(currentPath.value)
    showFlashHint(result.message)
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    remoteBusy.value = false
  }
}

function cancelDeleteDialog() {
  deleteDialogOpen.value = false
  deleteDialogPaths.value = []
}

async function confirmDeleteDialog(recycle: boolean) {
  if (!host.value || remoteBusy.value) return
  const paths = deleteDialogPaths.value
  if (paths.length === 0) {
    cancelDeleteDialog()
    return
  }
  deleteDialogOpen.value = false
  deleteDialogPaths.value = []
  remoteBusy.value = true
  const action = recycle ? 'delete_recycle' : 'delete_permanent'
  try {
    const result = await remotePcFileOp(
      host.value,
      props.pc.port,
      action,
      paths,
      currentPath.value,
      '',
    )
    selectedItems.value = new Map()
    await loadDirectory(currentPath.value)
    showFlashHint(result.message)
  } catch (e) {
    const err = formatInvokeError(e)
    if (recycle && /delete_recycle|未知|unsupported|不支援/i.test(err)) {
      showFlashHint('PC 配套端需更新為支援資源回收桶的版本，或改選「永久刪除」。', 3200)
    } else {
      showFlashHint(err, 2800)
    }
  } finally {
    remoteBusy.value = false
  }
}

async function confirmRenameDialog() {
  if (!host.value || remoteBusy.value || !renameDialogPath.value) return
  const oldName = baseName(renameDialogPath.value)
  const finalName = resolveRenameFinalName(
    oldName,
    renameDialogInput.value,
    renameDialogIsDir.value,
  )
  if (!finalName) {
    showFlashHint('名稱不可為空')
    return
  }
  const current = renameDialogPath.value
  renameDialogOpen.value = false
  remoteBusy.value = true
  try {
    const result = await remotePcFileOp(
      host.value,
      props.pc.port,
      'rename',
      [current],
      currentPath.value,
      finalName,
    )
    selectedItems.value.delete(current)
    await loadDirectory(currentPath.value)
    showFlashHint(result.message)
    remoteBusy.value = false
    renameDialogPath.value = ''
    advanceRenameQueue()
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
    remoteBusy.value = false
    // 失敗時重開目前項目，讓使用者可改名重試或取消／全部終止
    openRenameDialog(current)
  }
}

onMounted(async () => {
  document.addEventListener('click', onDocumentClickForLayoutMenu)
  document.addEventListener('click', onDocumentClickForActionMenus)
  void loadDirectory('')
  unlistenTransfer = await listen<RemoteTransferProgressEvent>(
    'remote-transfer-progress-event',
    (ev) => {
      transferProgress.value = ev.payload
      const diag = ev.payload.detailLog?.trim()
      if (diag) {
        appendPcDetailLogDelta(diag, ev.payload.logSeq)
      }
      const msg = ev.payload.message?.trim()
      // 每秒速度訊息不入日誌；只在階段或檔案序號變化時記一筆
      const statusKey = `${ev.payload.phase}|${ev.payload.fileIndex}`
      if (
        msg &&
        statusKey !== lastTransferStatusKey &&
        !ev.payload.finished &&
        msg !== '準備貼上…' &&
        msg !== '準備跨 PC 貼上…' &&
        msg !== '正在計算大小…' &&
        msg !== 'scanning size...'
      ) {
        lastTransferStatusKey = statusKey
        if (!diag || !diag.includes(msg)) {
          appendTransferLog(`狀態：${msg}`)
        }
      }
      if (ev.payload.finished) {
        remoteBusy.value = false
        appendTransferLog(ev.payload.message)
        if (ev.payload.error || ev.payload.phase === 'error') {
          console.error('[remote-transfer]', ev.payload)
        }
      }
    },
  )
})

onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClickForLayoutMenu)
  document.removeEventListener('click', onDocumentClickForActionMenus)
  unlistenTransfer?.()
  stopOverlayElapsedTimer()
  if (flashHintTimer != null) {
    window.clearTimeout(flashHintTimer)
  }
})

function pcIdentity(pc: RemotePcListItem): string {
  const h = pc.connectedHost ?? pc.hosts[0] ?? ''
  return `${h}:${pc.port}`
}

watch(
  () => pcIdentity(props.pc),
  (nextId, prevId) => {
    if (prevId !== undefined && nextId !== prevId) {
      currentPath.value = ''
      selectedItems.value = new Map()
      void loadDirectory('')
    }
  },
)
</script>

<template>
  <div class="remote-browse">
    <div class="remote-browse-header">
      <div class="remote-browse-actions">
        <div class="remote-browse-actions-left">
          <button
            type="button"
            class="tool tool--ghost remote-browse-select-all"
            :disabled="loading || sortedEntries.length === 0 || remoteBusy"
            @click="toggleSelectAll"
          >
            {{ allCurrentSelected ? '取消全選' : '全選' }}
          </button>
          <div class="remote-upload-menu-wrap">
            <button
              ref="opMenuBtnRef"
              type="button"
              class="tool tool--ghost remote-browse-op"
              :disabled="remoteBusy"
              @click.stop="toggleOpMenu"
            >
              操作 ▾
            </button>
          </div>
        </div>
        <div class="remote-browse-actions-right">
          <button
            type="button"
            class="tool tool--ghost"
            :disabled="remoteBusy || selectedCount === 0"
            @click="startStreamAuto"
          >
            串流
          </button>
          <div class="remote-upload-menu-wrap">
            <button
              ref="transferMenuBtnRef"
              type="button"
              class="tool tool--ghost"
              :disabled="remoteBusy"
              @click.stop="toggleTransferMenu"
            >
              傳輸 ▾
            </button>
          </div>
        </div>
      </div>
      <div class="remote-browse-title">
        <span class="remote-browse-pc">{{ displayPcName }}</span>
        <span class="remote-browse-path">{{ pathLabel }}</span>
        <span class="remote-browse-hint"
          >勾選後「操作」剪下/複製/貼上/刪除/重新命名/新建資料夾/收藏此漫畫/收藏此影片；「傳輸」上傳/下載；勾選影片、ZIP/CBZ
          或資料夾後按「串流」（自動辨識影片／漫畫；漫畫閱讀需 PC remote_api≥8）</span
        >
      </div>
    </div>

    <div class="remote-browse-body">
      <p v-if="pcShareRootsOutdated" class="remote-browse-pc-update">
        PC 遠端服務過舊（remote_api={{ browse?.remoteApi ?? '?' }}，需 ≥6）。請在 PC 執行最新版
        Nas-Manager-Windows，並按「重新啟動服務」，路徑才會顯示 H:\、I:\ 等。
      </p>
      <p v-if="loading" class="remote-browse-status">載入中…</p>
      <p v-else-if="error" class="remote-browse-error">{{ error }}</p>
      <ul v-else-if="!isGridView" class="remote-browse-list">
        <li
          v-for="entry in sortedEntries"
          :key="entry.name"
          class="remote-browse-item"
          :class="{
            'remote-browse-item--dir': entry.isDir,
            'remote-browse-item--video': !entry.isDir && isVideoFile(entry.name),
            'remote-browse-item--selected': isSelected(entry),
          }"
        >
          <div class="remote-browse-row">
            <button
              type="button"
              class="remote-browse-check-btn"
              :aria-label="isSelected(entry) ? '取消勾選' : '勾選'"
              :disabled="remoteBusy"
              @click="toggleSelect(entry)"
            >
              <span class="remote-browse-check">{{ isSelected(entry) ? '☑' : '☐' }}</span>
            </button>
            <button
              type="button"
              class="remote-browse-main"
              :disabled="remoteBusy && entry.isDir"
              @click="onMainEntryClick(entry)"
            >
              <span class="remote-browse-icon">{{
                entry.isDir ? '📁' : isVideoFile(entry.name) ? '🎬' : '📄'
              }}</span>
              <span class="remote-browse-name">{{ entryLabel(entry) }}</span>
              <div v-if="entryHasDiskCapacity(entry)" class="remote-browse-disk-cap">
                <div class="remote-browse-disk-cap-inner">
                  <div class="remote-browse-disk-bar" aria-hidden="true">
                    <div
                      class="remote-browse-disk-bar-fill"
                      :style="{ width: `${entryDiskUsedPercent(entry)}%` }"
                    />
                  </div>
                  <span class="remote-browse-disk-text">{{ entryDiskCapacityLabel(entry) }}</span>
                </div>
              </div>
              <span v-else-if="entrySecondaryLabel(entry)" class="remote-browse-size">
                {{ entrySecondaryLabel(entry) }}
              </span>
            </button>
          </div>
        </li>
        <li
          v-if="!loading && !error && sortedEntries.length === 0"
          class="remote-browse-empty"
        >
          （空資料夾）
        </li>
      </ul>
      <div
        v-else
        class="remote-browse-grid"
        :style="{ gridTemplateColumns: `repeat(${gridCols}, minmax(0, 1fr))` }"
      >
        <div
          v-for="entry in sortedEntries"
          :key="entry.name"
          class="remote-browse-grid-item"
          :class="{
            'remote-browse-grid-item--dir': entry.isDir,
            'remote-browse-grid-item--selected': isSelected(entry),
          }"
        >
          <button
            type="button"
            class="remote-browse-grid-check"
            :aria-label="isSelected(entry) ? '取消勾選' : '勾選'"
            :disabled="remoteBusy"
            @click.stop="toggleSelect(entry)"
          >
            <span class="remote-browse-check">{{ isSelected(entry) ? '☑' : '☐' }}</span>
          </button>
          <button
            type="button"
            class="remote-browse-grid-main"
            :disabled="remoteBusy && entry.isDir"
            @click="onMainEntryClick(entry)"
          >
            <RemoteBrowseEntryThumb
              :host="host"
              :port="pc.port"
              :rel-path="entryPath(entry)"
              :name="entry.name"
              :is-dir="entry.isDir"
              :remote-api="browse?.remoteApi"
            />
            <span class="remote-browse-grid-name" :title="entryLabel(entry)">{{
              entryLabel(entry)
            }}</span>
            <div v-if="entryHasDiskCapacity(entry)" class="remote-browse-disk-cap remote-browse-disk-cap--grid">
              <div class="remote-browse-disk-cap-inner">
                <div class="remote-browse-disk-bar" aria-hidden="true">
                  <div
                    class="remote-browse-disk-bar-fill"
                    :style="{ width: `${entryDiskUsedPercent(entry)}%` }"
                  />
                </div>
                <span class="remote-browse-disk-text">{{ entryDiskCapacityLabel(entry) }}</span>
              </div>
            </div>
            <span v-else-if="entrySecondaryLabel(entry)" class="remote-browse-grid-size">{{
              entrySecondaryLabel(entry)
            }}</span>
          </button>
        </div>
        <p
          v-if="!loading && !error && sortedEntries.length === 0"
          class="remote-browse-empty remote-browse-empty--grid"
        >
          （空資料夾）
        </p>
      </div>
    </div>

    
    <Teleport to="body">
      <div
        v-if="opMenuOpen"
        class="remote-upload-menu remote-op-menu remote-upload-menu--float"
        :style="opMenuFloatStyle"
        @click.stop
      >
        <button
          type="button"
          :disabled="remoteBusy || selectedCount === 0"
          @click="runFileOp('cut')"
        >
          剪下
        </button>
        <button
          type="button"
          :disabled="remoteBusy || selectedCount === 0"
          @click="runFileOp('copy')"
        >
          複製
        </button>
        <button type="button" :disabled="remoteBusy" @click="runFileOp('paste')">
          貼上
        </button>
        <button
          type="button"
          :disabled="remoteBusy || selectedCount === 0"
          @click="runFileOp('delete')"
        >
          刪除
        </button>
        <button
          type="button"
          :disabled="remoteBusy || selectedCount === 0"
          @click="runFileOp('rename')"
        >
          重新命名
        </button>
        <button type="button" :disabled="remoteBusy" @click="runFileOp('mkdir')">
          新建資料夾
        </button>
        <button
          type="button"
          :disabled="remoteBusy || selectedCount === 0"
          @click="startFavoriteComic"
        >
          收藏此漫畫
        </button>
        <button
          type="button"
          :disabled="remoteBusy || selectedCount === 0"
          @click="startFavoriteVideo"
        >
          收藏此影片
        </button>
      </div>
      <div
        v-if="transferMenuOpen"
        class="remote-upload-menu remote-upload-menu--float"
        :style="transferMenuFloatStyle"
        @click.stop
      >
        <button type="button" :disabled="remoteBusy" @click="openUploadPicker">上傳</button>
        <button type="button" :disabled="selectedCount === 0" @click="startDownload">
          下載{{ selectedCount > 0 ? ` (${selectedCount})` : '' }}
        </button>
      </div>
    </Teleport>

<Teleport to="#gm-remote-browse-foot-slot" :disabled="!dockFootEnabled">
      <div v-if="dockFootEnabled" class="remote-browse-foot remote-browse-foot--dock">
        <div class="remote-browse-foot-slot remote-browse-foot-slot--left">
          <button type="button" class="tool tool--ghost tool--foot" :disabled="remoteBusy" @click="emit('exit')">
            退出
          </button>
          <button
            type="button"
            class="tool tool--ghost tool--foot"
            :disabled="!currentPath || remoteBusy"
            @click="goUp"
          >
            上一層
          </button>
        </div>
        <div class="remote-browse-foot-slot remote-browse-foot-slot--right">
          <div ref="layoutDropdownRef" class="remote-layout-menu-wrap">
            <button
              type="button"
              class="tool tool--ghost tool--foot tool--layout"
              :disabled="loading"
              :aria-expanded="layoutMenuOpen"
              @click.stop="toggleLayoutMenu"
            >
              顯示方式 ▴
            </button>
            <div v-show="layoutMenuOpen" class="remote-layout-menu" @click.stop>
              <button
                v-for="opt in REMOTE_BROWSE_LAYOUT_OPTIONS"
                :key="opt.key"
                type="button"
                class="remote-layout-menu-item"
                :class="{ on: viewLayout === opt.key }"
                @click="chooseBrowseLayout(opt.key)"
              >
                {{ opt.label }}
              </button>
            </div>
          </div>
          <button
            type="button"
            class="tool tool--ghost tool--foot tool--sort"
            :disabled="loading || sortedEntries.length === 0"
            @click="toggleNameSort"
          >
            {{ sortToggleLabel }}
          </button>
        </div>
      </div>
    </Teleport>

    <div v-if="flashHint" class="remote-browse-flash-hint" role="status" aria-live="polite">
      {{ flashHint }}
    </div>

    <div v-if="uploadConflictOpen" class="remote-upload-conflict-overlay" @click.self="cancelUploadConflict">
      <div class="remote-upload-conflict-panel" @click.stop>
        <p class="remote-upload-conflict-title">PC 上已有同名檔案</p>
        <p class="remote-upload-conflict-hint">以下路徑將衝突，請選擇處理方式：</p>
        <ul class="remote-upload-conflict-list">
          <li v-for="p in uploadConflictList" :key="p">{{ p }}</li>
        </ul>
        <div class="remote-upload-conflict-actions">
          <button type="button" class="tool tool--primary" @click="resolveUploadConflict('overwrite')">
            覆蓋
          </button>
          <button type="button" class="tool tool--ghost" @click="resolveUploadConflict('keep_both')">
            都保留
          </button>
          <button type="button" class="tool tool--ghost" @click="cancelUploadConflict">取消</button>
        </div>
      </div>
    </div>

    <div v-if="deleteDialogOpen" class="remote-upload-conflict-overlay" @click.self="cancelDeleteDialog">
      <div class="remote-upload-conflict-panel" @click.stop>
        <p class="remote-upload-conflict-title">刪除確認</p>
        <p class="remote-upload-conflict-hint">
          將刪除 PC 上 {{ deleteDialogPaths.length }} 項，請選擇刪除方式：
        </p>
        <div class="remote-upload-conflict-actions remote-delete-actions">
          <button type="button" class="tool tool--primary" @click="confirmDeleteDialog(true)">
            移至資源回收桶
          </button>
          <button type="button" class="tool tool--ghost" @click="confirmDeleteDialog(false)">
            永久刪除
          </button>
          <button type="button" class="tool tool--ghost" @click="cancelDeleteDialog">取消</button>
        </div>
      </div>
    </div>

    <div v-if="mkdirDialogOpen" class="remote-upload-conflict-overlay" @click.self="cancelMkdirDialog">
      <div class="remote-upload-conflict-panel remote-rename-panel" @click.stop>
        <p class="remote-upload-conflict-title">新建資料夾</p>
        <p class="remote-upload-conflict-hint">將在目前 PC 目錄建立資料夾：{{ pathLabel }}</p>
        <input
          ref="mkdirInputRef"
          v-model="mkdirDialogInput"
          class="remote-rename-input"
          type="text"
          maxlength="255"
          autocomplete="off"
          placeholder="資料夾名稱"
          @keydown.enter.prevent="confirmMkdirDialog"
        />
        <div class="remote-upload-conflict-actions">
          <button type="button" class="tool tool--primary" @click="confirmMkdirDialog">建立</button>
          <button type="button" class="tool tool--ghost" @click="cancelMkdirDialog">取消</button>
        </div>
      </div>
    </div>

    <div v-if="renameDialogOpen" class="remote-upload-conflict-overlay" @click.self="cancelRenameDialog">
      <div class="remote-upload-conflict-panel remote-rename-panel" @click.stop>
        <p class="remote-upload-conflict-title">
          重新命名
          <span v-if="renameQueueTotal > 1" class="remote-rename-progress"
            >（{{ renameQueueIndex }} / {{ renameQueueTotal }}）</span
          >
        </p>
        <p class="remote-upload-conflict-hint">
          未輸入副檔名時將保留原名副檔名（例：`12345.zip` → `123` 會變成 `123.zip`）。「取消」略過目前這項；「全部終止命名」結束後續。
        </p>
        <input
          ref="renameInputRef"
          v-model="renameDialogInput"
          class="remote-rename-input"
          type="text"
          maxlength="255"
          autocomplete="off"
          @keydown.enter.prevent="confirmRenameDialog"
        />
        <div class="remote-upload-conflict-actions remote-rename-actions">
          <button type="button" class="tool tool--primary" @click="confirmRenameDialog">確認</button>
          <button type="button" class="tool tool--ghost" @click="cancelRenameDialog">取消</button>
          <button
            v-if="renameQueueTotal > 1 || renameQueue.length > 0"
            type="button"
            class="tool tool--ghost"
            @click="abortAllRenameDialog"
          >
            全部終止命名
          </button>
        </div>
      </div>
    </div>

    <div v-if="uploadPickerOpen" class="remote-upload-conflict-overlay" @click.self="closeUploadPicker">
      <div class="remote-upload-conflict-panel remote-upload-picker-panel" @click.stop>
        <p class="remote-upload-conflict-title">上傳</p>
        <p class="remote-upload-conflict-hint">可多次加入檔案與資料夾，再一起上傳到目前目錄。</p>
        <div class="remote-upload-picker-actions">
          <button type="button" class="tool tool--ghost" :disabled="uploadPickerBusy" @click="addUploadFiles">
            加入檔案
          </button>
          <button type="button" class="tool tool--ghost" :disabled="uploadPickerBusy" @click="addUploadFolder">
            加入資料夾
          </button>
        </div>
        <ul v-if="uploadSources.length" class="remote-upload-source-list">
          <li v-for="(src, idx) in uploadSources" :key="src.uri + src.kind" class="remote-upload-source-item">
            <span class="remote-upload-source-kind">{{ src.kind === 'folder' ? '資料夾' : '檔案' }}</span>
            <span class="remote-upload-source-name" :title="src.uri">{{ src.label }}</span>
            <button type="button" class="tool tool--ghost remote-upload-source-remove" @click="removeUploadSource(idx)">
              移除
            </button>
          </li>
        </ul>
        <p v-else class="remote-upload-conflict-hint">尚未加入來源</p>
        <div class="remote-upload-conflict-actions">
          <button
            type="button"
            class="tool tool--primary"
            :disabled="uploadPickerBusy || uploadSources.length === 0"
            @click="confirmUploadPicker"
          >
            開始上傳{{ uploadSources.length ? ` (${uploadSources.length})` : '' }}
          </button>
          <button type="button" class="tool tool--ghost" :disabled="uploadPickerBusy" @click="closeUploadPicker">
            取消
          </button>
        </div>
      </div>
    </div>

    <div v-if="transferProgress" class="remote-transfer-overlay">
      <div class="remote-transfer-panel">
        <p class="remote-transfer-title">
          {{ transferOverlayTitle }}
        </p>
        <p class="remote-transfer-msg remote-transfer-msg--selectable">{{ transferErrorMessage }}</p>
        <div v-if="showTransferLog" class="remote-transfer-log-wrap">
          <textarea
            ref="transferLogRef"
            class="remote-transfer-log"
            readonly
            :value="transferLogCopyText"
            aria-label="傳輸結果日誌"
          />
          <div class="remote-transfer-log-actions">
            <button
              type="button"
              class="tool tool--ghost remote-transfer-copy"
              @click="copyTransferLog"
            >
              複製日誌
            </button>
            <span v-if="copyLogHint" class="remote-transfer-copy-hint">{{ copyLogHint }}</span>
          </div>
        </div>
        <p v-if="transferProgress.fileCount > 0" class="remote-transfer-detail">
          檔案 {{ transferProgress.fileIndex }} / {{ transferProgress.fileCount }}
        </p>
        <p v-if="transferProgress.bytesTotal > 0" class="remote-transfer-detail">
          {{ formatSize(transferProgress.bytesDone) }} / {{ formatSize(transferProgress.bytesTotal) }}
          <span v-if="transferDisplaySpeedBps > 0"> · {{ formatSpeed(transferDisplaySpeedBps) }}</span>
        </p>
        <p v-if="transferStartedAt > 0" class="remote-transfer-detail">{{ transferElapsedText }}</p>
        <div
          v-if="transferProgress.bytesTotal > 0"
          class="remote-transfer-bar"
        >
          <div
            class="remote-transfer-bar-fill"
            :style="{
              width: `${Math.min(100, (transferProgress.bytesDone / transferProgress.bytesTotal) * 100)}%`,
            }"
          />
        </div>
        <button
          v-if="!transferProgress.finished"
          type="button"
          class="tool tool--ghost remote-transfer-cancel"
          @click="cancelActiveTransfer"
        >
          取消
        </button>
        <button
          v-if="transferProgress.finished"
          type="button"
          class="tool tool--primary remote-transfer-close"
          @click="closeTransferOverlay"
        >
          關閉
        </button>
      </div>
    </div>

    <StreamFavoriteAddDialog
      :open="favAddDialogOpen"
      :adapter="favAddDialogAdapter"
      :jobs="favAddDialogJobs"
      :job-groups="favAddDialogGroups"
      :default-name-fallback="favAddDialogFallback"
      @close="favAddDialogOpen = false; favAddDialogGroups = []"
      @done="onFavoriteAddDialogDone"
    />
  </div>
</template>

<style scoped>
.remote-browse {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 12px 14px 0;
  box-sizing: border-box;
  position: relative;
}

.remote-browse-header {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding-bottom: 10px;
  background: var(--gm-page-bg, #1a1a1a);
  border-bottom: 1px solid var(--gm-border, rgba(255, 255, 255, 0.1));
  position: relative;
  z-index: 30;
}

.remote-browse-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  padding-bottom: 8px;
}

.remote-browse-actions {
  display: flex;
  flex-wrap: nowrap;
  gap: 8px;
  align-items: center;
  justify-content: space-between;
}

.remote-browse-actions-left {
  display: flex;
  flex-wrap: nowrap;
  gap: 8px;
  align-items: center;
  flex-shrink: 0;
}

.remote-browse-select-all,
.remote-browse-op {
  flex-shrink: 0;
}

.remote-op-menu {
  left: 0;
  right: auto;
  min-width: 120px;
}

.remote-browse-select-all {
  margin-right: 0;
}

.remote-browse-actions-right {
  display: flex;
  flex-wrap: nowrap;
  gap: 8px;
  align-items: center;
  flex-shrink: 0;
}

.remote-browse-upload,
.remote-browse-transfer {
  flex-shrink: 0;
}

.remote-upload-menu-wrap {
  position: relative;
}

.remote-upload-menu--float {
  position: fixed;
  z-index: 10000;
  max-height: min(70vh, 480px);
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
}

.remote-upload-menu {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 80;
  min-width: 140px;
  padding: 6px;
  border-radius: 8px;
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.2));
  background: #252525;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.45);
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.remote-upload-menu button {
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: inherit;
  font-size: 13px;
  text-align: left;
}

.remote-upload-menu button:active {
  background: rgba(49, 130, 206, 0.25);
}

.remote-browse-flash-hint {
  position: fixed;
  left: 50%;
  bottom: calc(
    var(--gm-remote-foot-h, 44px) + var(--gm-bottom-tabs-h, 34px) + env(safe-area-inset-bottom, 0px) + 10px
  );
  z-index: 90;
  transform: translateX(-50%);
  max-width: min(92vw, 320px);
  padding: 10px 16px;
  border-radius: 8px;
  background: rgba(30, 30, 30, 0.92);
  border: 1px solid rgba(255, 255, 255, 0.18);
  color: #f0f0f0;
  font-size: 13px;
  line-height: 1.4;
  text-align: center;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
  pointer-events: none;
  animation: remote-browse-flash-in 0.15s ease-out;
}

@keyframes remote-browse-flash-in {
  from {
    opacity: 0;
    transform: translateX(-50%) translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }
}

.remote-upload-conflict-overlay {
  position: fixed;
  inset: 0;
  z-index: 110;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}

.remote-upload-conflict-panel {
  width: min(100%, 360px);
  max-height: 70vh;
  overflow: auto;
  padding: 16px;
  border-radius: 12px;
  background: #1a1a1a;
  border: 1px solid rgba(255, 255, 255, 0.15);
}

.remote-upload-conflict-title {
  margin: 0 0 6px;
  font-weight: 600;
  font-size: 16px;
}

.remote-upload-conflict-hint {
  margin: 0 0 8px;
  font-size: 13px;
  opacity: 0.8;
}

.remote-upload-conflict-list {
  margin: 0 0 12px;
  padding-left: 18px;
  font-size: 12px;
  line-height: 1.4;
  word-break: break-all;
  max-height: 160px;
  overflow-y: auto;
}

.remote-upload-conflict-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.remote-delete-actions {
  flex-wrap: nowrap;
  align-items: stretch;
}

.remote-delete-actions .tool {
  flex: 1 1 0;
  min-width: 0;
  white-space: nowrap;
}

.remote-delete-actions .tool:last-child {
  flex: 0 0 auto;
}

.remote-rename-panel {
  width: min(100%, 340px);
}

.remote-rename-input {
  width: 100%;
  box-sizing: border-box;
  margin: 0 0 12px;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: #252525;
  color: inherit;
  font-size: 15px;
}

/* bottom-dock flex 內，緊貼 nav.bottom-tabs（零空隙） */
.remote-browse-foot--dock {
  position: relative;
  z-index: 30;
  flex-shrink: 0;
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 14px 6px;
  margin: 0;
  border-top: 1px solid var(--gm-border, rgba(255, 255, 255, 0.12));
  background: var(--gm-page-bg, #1a1a1a);
  box-sizing: border-box;
}

.remote-browse-foot-slot {
  display: flex;
  flex-wrap: nowrap;
  gap: 6px;
  align-items: center;
  min-width: 0;
}

.remote-browse-foot-slot--left {
  flex: 1;
  justify-content: flex-start;
}

.remote-browse-foot-slot--right {
  flex-shrink: 0;
  justify-content: flex-end;
}

.tool--foot {
  padding: 5px 10px;
  font-size: 12px;
  white-space: nowrap;
}

.tool--sort {
  min-width: 3.2em;
  text-align: center;
}

.tool--layout {
  min-width: 4.8em;
  text-align: center;
}

.remote-layout-menu-wrap {
  position: relative;
}

.remote-layout-menu {
  position: absolute;
  right: 0;
  bottom: calc(100% + 6px);
  z-index: 80;
  min-width: 132px;
  padding: 4px 0;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  background: #2a2a2a;
  box-shadow: 0 -4px 20px rgba(0, 0, 0, 0.45);
  display: flex;
  flex-direction: column;
}

.remote-layout-menu-item {
  width: 100%;
  padding: 9px 14px;
  border: none;
  background: transparent;
  color: #eee;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.remote-layout-menu-item:hover,
.remote-layout-menu-item.on {
  background: rgba(49, 130, 206, 0.22);
}

.remote-layout-menu-item.on {
  color: #9fd0ff;
}

.remote-browse-grid {
  display: grid;
  gap: 10px 8px;
  padding: 2px 0 8px;
}

.remote-browse-grid-item {
  position: relative;
  min-width: 0;
  border-radius: 6px;
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.12));
  background: rgba(0, 0, 0, 0.15);
  overflow: hidden;
}

.remote-browse-grid-item--selected {
  border-color: rgba(49, 130, 206, 0.65);
  background: rgba(49, 130, 206, 0.12);
}

.remote-browse-grid-check {
  position: absolute;
  top: 4px;
  left: 4px;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.55);
  color: inherit;
  font-size: 14px;
  cursor: pointer;
}

.remote-browse-grid-main {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  width: 100%;
  padding: 8px 6px 6px;
  border: none;
  background: transparent;
  color: inherit;
  cursor: pointer;
  text-align: center;
}

.remote-browse-grid-name {
  margin-top: 6px;
  font-size: 11px;
  line-height: 1.3;
  max-height: 2.6em;
  overflow: hidden;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  word-break: break-all;
}

.remote-browse-grid-size {
  margin-top: 2px;
  font-size: 10px;
  line-height: 1.25;
  opacity: 0.65;
  word-break: break-all;
}

.remote-browse-empty--grid {
  grid-column: 1 / -1;
  margin: 0;
}

.remote-browse-title {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.remote-browse-pc {
  font-weight: 600;
  font-size: 15px;
}

.remote-browse-path {
  font-size: 12px;
  opacity: 0.75;
  word-break: break-all;
}

.remote-browse-pc-update {
  margin: 0 0 10px;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid #8a5a3a;
  background: rgba(120, 60, 20, 0.25);
  color: #ffb380;
  font-size: 12px;
  line-height: 1.45;
  word-break: break-word;
}

.remote-browse-hint {
  font-size: 12px;
  opacity: 0.7;
}

.remote-browse-status {
  margin: 0;
  font-size: 13px;
  opacity: 0.8;
}

.remote-browse-error {
  margin: 0;
  font-size: 13px;
  color: #feb2b2;
  line-height: 1.5;
}

.remote-browse-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.remote-browse-row {
  width: 100%;
  display: flex;
  align-items: stretch;
  gap: 0;
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.12));
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.15);
  overflow: hidden;
}

.remote-browse-item--selected .remote-browse-row {
  border-color: rgba(49, 130, 206, 0.6);
  background: rgba(49, 130, 206, 0.15);
}

.remote-browse-check-btn {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  padding: 0;
  border: none;
  border-right: 1px solid var(--gm-border, rgba(255, 255, 255, 0.1));
  background: transparent;
  color: inherit;
}

.remote-browse-main {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 14px;
  text-align: left;
}

.remote-browse-check {
  flex-shrink: 0;
  width: 1.2em;
  text-align: center;
  opacity: 0.9;
  font-size: 16px;
}

.remote-browse-icon {
  flex-shrink: 0;
  width: 1.4em;
  text-align: center;
}

.remote-browse-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.remote-browse-size {
  flex-shrink: 0;
  font-size: 11px;
  opacity: 0.65;
}

.remote-browse-disk-cap {
  flex: 0 0 33.333%;
  max-width: 33.333%;
  min-width: 0;
  display: flex;
  flex-direction: column;
  align-items: stretch;
  align-self: center;
  padding-left: 6px;
  box-sizing: border-box;
}

.remote-browse-disk-cap-inner {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 4px;
  width: 100%;
  max-width: 100%;
}

.remote-browse-disk-cap--grid {
  flex: none;
  width: 100%;
  max-width: 100%;
  margin-top: 6px;
  padding-left: 0;
  align-items: center;
}

.remote-browse-disk-bar {
  width: 100%;
  flex-shrink: 0;
  height: 9px;
  border: 1px solid rgba(100, 149, 237, 0.45);
  background: rgba(255, 255, 255, 0.1);
  overflow: hidden;
}

.remote-browse-disk-bar-fill {
  height: 100%;
  background: linear-gradient(180deg, #5b9bd5 0%, #4a90d9 100%);
}

.remote-browse-disk-text {
  font-size: 10px;
  line-height: 1.2;
  opacity: 0.78;
  white-space: nowrap;
  text-align: right;
}

.remote-browse-disk-cap--grid .remote-browse-disk-text {
  text-align: center;
  font-size: 10px;
}

.remote-browse-empty {
  padding: 16px;
  text-align: center;
  opacity: 0.6;
  font-size: 13px;
}

.tool--ghost {
  padding: 6px 12px;
  border-radius: 8px;
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.2));
  background: transparent;
  color: inherit;
  font-size: 13px;
}

.tool--accent {
  padding: 6px 12px;
  border-radius: 8px;
  border: none;
  background: #3182ce;
  color: #fff;
  font-size: 13px;
}

.tool--primary {
  padding: 8px 16px;
  border-radius: 8px;
  border: none;
  background: #3182ce;
  color: #fff;
}

.remote-transfer-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}

.remote-transfer-panel {
  width: min(100%, 360px);
  padding: 18px;
  border-radius: 12px;
  background: #1a1a1a;
  border: 1px solid rgba(255, 255, 255, 0.15);
}

.remote-transfer-title {
  margin: 0 0 8px;
  font-weight: 600;
  font-size: 16px;
}

.remote-transfer-msg {
  margin: 0 0 6px;
  font-size: 13px;
  line-height: 1.4;
  word-break: break-all;
}

.remote-transfer-msg--selectable {
  user-select: text;
  -webkit-user-select: text;
}

.remote-transfer-log-wrap {
  margin: 8px 0 0;
}

.remote-transfer-log {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 0;
  padding: 8px;
  max-height: 180px;
  min-height: 72px;
  overflow: auto;
  resize: none;
  font-family: ui-monospace, monospace;
  font-size: 10px;
  line-height: 1.35;
  white-space: pre-wrap;
  word-break: break-all;
  background: rgba(0, 0, 0, 0.35);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 6px;
  color: #cbd5e0;
  user-select: text;
  -webkit-user-select: text;
}

.remote-transfer-log-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
  flex-wrap: wrap;
}

.remote-transfer-copy {
  flex-shrink: 0;
  padding: 5px 12px;
  font-size: 12px;
}

.remote-transfer-copy-hint {
  font-size: 12px;
  opacity: 0.75;
}

.remote-transfer-detail {
  margin: 0 0 4px;
  font-size: 12px;
  opacity: 0.8;
}

.remote-transfer-bar {
  height: 8px;
  margin: 12px 0;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.1);
  overflow: hidden;
}

.remote-transfer-bar-fill {
  height: 100%;
  background: #3182ce;
  transition: width 0.2s ease;
}

.remote-transfer-close,
.remote-transfer-cancel {
  width: 100%;
  margin-top: 8px;
}

.remote-rename-progress {
  font-weight: 400;
  opacity: 0.75;
  font-size: 13px;
}

.remote-rename-actions {
  flex-wrap: wrap;
}

.remote-upload-picker-panel {
  max-width: min(440px, 94vw);
}

.remote-upload-picker-actions {
  display: flex;
  gap: 8px;
  margin-bottom: 10px;
}

.remote-upload-source-list {
  list-style: none;
  margin: 0 0 12px;
  padding: 0;
  max-height: 220px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.remote-upload-source-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.05);
}

.remote-upload-source-kind {
  flex-shrink: 0;
  font-size: 11px;
  opacity: 0.7;
}

.remote-upload-source-name {
  flex: 1;
  min-width: 0;
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.remote-upload-source-remove {
  flex-shrink: 0;
  padding: 2px 8px;
  font-size: 12px;
}
</style>
