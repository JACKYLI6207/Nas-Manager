<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  cancelRemotePcTransfer,
  listRemotePcDirectory,
  pickRemoteTransferDestination,
  pickRemoteUploadFile,
  pickRemoteUploadFolder,
  planRemotePcUpload,
  remotePcFileOp,
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
} from '../videoStreamStore'
import {
  gridColsForRemoteBrowseLayout,
  isRemoteBrowseGridLayout,
  loadSavedRemoteBrowseLayout,
  REMOTE_BROWSE_LAYOUT_OPTIONS,
  saveRemoteBrowseLayout,
  type RemoteBrowseLayout,
} from '../remoteBrowseLayout'
import { isComicZipFileName, isVideoFileName } from '../remoteBrowseMedia'
import RemoteBrowseEntryThumb from './RemoteBrowseEntryThumb.vue'

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
const renameDialogOpen = ref(false)
const renameDialogInput = ref('')
const renameDialogPath = ref('')
const renameDialogIsDir = ref(false)
const renameInputRef = ref<HTMLInputElement | null>(null)
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
  transferDebugLog.value = [...transferDebugLog.value, `[${ts}] ${line}`]
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
  if (!p.finished) {
    return p.phase === 'uploading' || p.message.includes('上傳') ? '上傳中' : '下載中'
  }
  if (p.phase === 'cancelled') {
    return p.message.includes('上傳') ? '上傳已取消' : '下載已取消'
  }
  if (p.phase === 'partial') {
    return p.message.includes('上傳') ? '上傳完成（部分失敗）' : '下載完成（部分失敗）'
  }
  if (p.error || p.phase === 'error') {
    return p.message.includes('上傳') ? '上傳失敗' : '下載失敗'
  }
  return p.message.includes('上傳') ? '上傳完成' : '下載完成'
})

const transferDebugText = computed(() => {
  const parts = [...transferDebugLog.value]
  const detail = transferProgress.value?.detailLog
  if (detail && !parts.includes(detail)) {
    parts.push(detail)
  }
  return parts.join('\n')
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
    const cmp = a.name.localeCompare(b.name, 'zh-Hant', { numeric: true, sensitivity: 'base' })
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

/** 列表右側：根目錄磁碟「剩餘/總容量」或檔案/資料夾大小 */
function entrySecondaryLabel(entry: RemotePcDirEntry): string {
  if (
    isBrowseRoot.value &&
    entry.isDir &&
    entry.diskFreeBytes != null &&
    entry.diskTotalBytes != null
  ) {
    return `${formatSize(entry.diskFreeBytes)} / ${formatSize(entry.diskTotalBytes)}`
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
    browse.value = await listRemotePcDirectory(host.value, props.pc.port, path)
    currentPath.value = browse.value.path
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

function collectSelectedVideoJobs() {
  const h = host.value
  if (!h) return []
  const jobs: { host: string; port: number; relPath: string; title: string }[] = []
  for (const path of selectedItems.value.keys()) {
    const name = baseName(path)
    if (!isVideoFile(name)) continue
    jobs.push({
      host: h,
      port: props.pc.port,
      relPath: path,
      title: name,
    })
  }
  return jobs
}

function collectSelectedComicJobs() {
  const h = host.value
  if (!h) return []
  const jobs: { host: string; port: number; relPath: string; title: string }[] = []
  for (const path of selectedItems.value.keys()) {
    const name = baseName(path)
    if (!isComicZipFile(name)) continue
    jobs.push({
      host: h,
      port: props.pc.port,
      relPath: path,
      title: name,
    })
  }
  return jobs
}

function startStreamRead() {
  closeActionMenus()
  const h = host.value
  if (!h) {
    showFlashHint('未連線 PC')
    return
  }
  const jobs = collectSelectedComicJobs()
  if (jobs.length === 0) {
    showFlashHint('請勾選 ZIP/CBZ 漫畫檔')
    return
  }
  queueRemoteComicStreamPlay(jobs)
  comicStreamLog(`來源 PC：${h}:${props.pc.port}`)
  if (jobs.length === 1) {
    activateComicStreamQueueJob(jobs[0]!, jobs)
  } else {
    requestNavigateToComicStream()
  }
}

function startStreamPlay() {
  closeActionMenus()
  const h = host.value
  if (!h) {
    showFlashHint('未連線 PC')
    return
  }
  const jobs = collectSelectedVideoJobs()
  if (jobs.length === 0) {
    showFlashHint('請勾選要播放的影片檔')
    return
  }
  queueRemoteStreamPlay(jobs)
  streamLog(`來源 PC：${h}:${props.pc.port}`)
  requestNavigateToVideoTab()
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

async function startUpload(kind: 'file' | 'folder') {
  transferMenuOpen.value = false
  streamMenuOpen.value = false
  opMenuOpen.value = false
  if (!host.value || remoteBusy.value) return
  try {
    const sourceUri =
      kind === 'file' ? await pickRemoteUploadFile() : await pickRemoteUploadFolder()
    if (!sourceUri) return
    const plan = await planRemotePcUpload(
      host.value,
      props.pc.port,
      currentPath.value,
      sourceUri,
      kind,
    )
    if (plan.files.length === 0) return
    let policy: RemoteUploadConflictPolicy = 'overwrite'
    if (plan.conflicts.length > 0) {
      const chosen = await askUploadConflict(plan.conflicts)
      if (!chosen) return
      policy = chosen
    }
    remoteBusy.value = true
    transferDebugLog.value = []
    appendTransferLog(`開始上傳 → PC ${host.value}:${props.pc.port}（${pathLabel.value}）`)
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
    await uploadRemotePcFiles(host.value, props.pc.port, plan.files, policy)
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
  transferDebugLog.value = []
  appendTransferLog(`開始下載 → PC ${host.value}:${props.pc.port}`)
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
  transferDebugLog.value = []
  copyLogHint.value = ''
}

async function cancelActiveTransfer() {
  if (!transferProgress.value || transferProgress.value.finished) return
  appendTransferLog('使用者要求取消傳輸…')
  try {
    await cancelRemotePcTransfer()
  } catch (e) {
    appendTransferLog(`取消指令失敗：${formatInvokeError(e)}`)
  }
}

async function runFileOp(action: RemotePcFileOpAction) {
  opMenuOpen.value = false
  transferMenuOpen.value = false
  streamMenuOpen.value = false
  if (!host.value || remoteBusy.value) return
  const paths = Array.from(selectedItems.value.keys())

  if (action === 'rename') {
    if (paths.length !== 1) {
      showFlashHint('重新命名請只選一項')
      return
    }
    openRenameDialog(paths[0]!)
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

  remoteBusy.value = true
  try {
    if (action === 'paste') {
      // 貼上至目前 PC 目錄
    } else if (paths.length === 0) {
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

function cancelRenameDialog() {
  renameDialogOpen.value = false
  renameDialogPath.value = ''
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
  renameDialogOpen.value = false
  remoteBusy.value = true
  try {
    const result = await remotePcFileOp(
      host.value,
      props.pc.port,
      'rename',
      [renameDialogPath.value],
      currentPath.value,
      finalName,
    )
    selectedItems.value = new Map()
    await loadDirectory(currentPath.value)
    showFlashHint(result.message)
  } catch (e) {
    showFlashHint(formatInvokeError(e), 2800)
  } finally {
    remoteBusy.value = false
    renameDialogPath.value = ''
  }
}

onMounted(async () => {
  document.addEventListener('click', onDocumentClickForLayoutMenu)
  void loadDirectory('')
  unlistenTransfer = await listen<RemoteTransferProgressEvent>(
    'remote-transfer-progress-event',
    (ev) => {
      transferProgress.value = ev.payload
      if (ev.payload.finished) {
        remoteBusy.value = false
        appendTransferLog(ev.payload.message)
        if (ev.payload.detailLog) {
          appendTransferLog(ev.payload.detailLog)
        }
        if (ev.payload.error || ev.payload.phase === 'error') {
          console.error('[remote-transfer]', ev.payload)
        }
      }
    },
  )
})

onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClickForLayoutMenu)
  unlistenTransfer?.()
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
              type="button"
              class="tool tool--ghost remote-browse-op"
              :disabled="remoteBusy"
              @click.stop="
                () => {
                  const next = !opMenuOpen
                  closeActionMenus()
                  opMenuOpen = next
                }
              "
            >
              操作 ▾
            </button>
            <div v-if="opMenuOpen" class="remote-upload-menu remote-op-menu" @click.stop>
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
                :disabled="remoteBusy || selectedCount !== 1"
                @click="runFileOp('rename')"
              >
                重新命名
              </button>
              <button type="button" :disabled="remoteBusy" @click="runFileOp('mkdir')">
                新建資料夾
              </button>
            </div>
          </div>
        </div>
        <div class="remote-browse-actions-right">
          <div class="remote-upload-menu-wrap">
            <button
              type="button"
              class="tool tool--ghost"
              :disabled="remoteBusy"
              @click.stop="
                () => {
                  const next = !streamMenuOpen
                  closeActionMenus()
                  streamMenuOpen = next
                }
              "
            >
              串流 ▾
            </button>
            <div v-if="streamMenuOpen" class="remote-upload-menu" @click.stop>
              <button type="button" @click="startStreamRead">串流閱讀</button>
              <button type="button" @click="startStreamPlay">串流播放</button>
            </div>
          </div>
          <div class="remote-upload-menu-wrap">
            <button
              type="button"
              class="tool tool--ghost"
              :disabled="remoteBusy"
              @click.stop="
                () => {
                  const next = !transferMenuOpen
                  closeActionMenus()
                  transferMenuOpen = next
                }
              "
            >
              傳輸 ▾
            </button>
            <div v-if="transferMenuOpen" class="remote-upload-menu" @click.stop>
              <button type="button" @click="startUpload('file')">上傳檔案</button>
              <button type="button" @click="startUpload('folder')">上傳資料夾</button>
              <button type="button" :disabled="selectedCount === 0" @click="startDownload">
                下載{{ selectedCount > 0 ? ` (${selectedCount})` : '' }}
              </button>
            </div>
          </div>
        </div>
      </div>
      <div class="remote-browse-title">
        <span class="remote-browse-pc">{{ displayPcName }}</span>
        <span class="remote-browse-path">{{ pathLabel }}</span>
        <span class="remote-browse-hint"
          >勾選後「操作」剪下/複製/貼上/刪除/重新命名/新建資料夾；「傳輸」上傳/下載；勾選 ZIP/CBZ 後「串流 →
          串流閱讀」、勾選影片後「串流 → 串流播放」（漫畫閱讀需 PC remote_api≥8）</span
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
              <span v-if="entrySecondaryLabel(entry)" class="remote-browse-size">
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
            <span v-if="entrySecondaryLabel(entry)" class="remote-browse-grid-size">{{
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
        <p class="remote-upload-conflict-title">重新命名</p>
        <p class="remote-upload-conflict-hint">未輸入副檔名時將保留原名副檔名（例：`12345.zip` → `123` 會變成 `123.zip`）</p>
        <input
          ref="renameInputRef"
          v-model="renameDialogInput"
          class="remote-rename-input"
          type="text"
          maxlength="255"
          autocomplete="off"
          @keydown.enter.prevent="confirmRenameDialog"
        />
        <div class="remote-upload-conflict-actions">
          <button type="button" class="tool tool--primary" @click="confirmRenameDialog">確認</button>
          <button type="button" class="tool tool--ghost" @click="cancelRenameDialog">取消</button>
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
          <span v-if="transferProgress.speedBps > 0"> · {{ formatSpeed(transferProgress.speedBps) }}</span>
        </p>
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
  z-index: 2;
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

.remote-upload-menu {
  position: absolute;
  right: 0;
  top: calc(100% + 6px);
  z-index: 40;
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
  z-index: 50;
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
</style>
