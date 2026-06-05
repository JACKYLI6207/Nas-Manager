import { reactive, ref } from 'vue'
import type { LocalReaderPage, LocalReaderSource } from './api'
import { decodeFolderDisplayLabel } from './readerDisplayName'

const FOLDER_STORAGE_KEY = 'gm-android-local-read-folder-v2'
const PROGRESS_BY_FOLDER_KEY = 'gm-android-local-read-folder-progress-v2'

export type LocalReadSessionKind = 'zip' | 'folder' | null

/**
 * 對齊 Perfect Viewer bookhistory2：relative_path + read_page + total_page + scrolly
 * 單一斷點，上下滑與拖曳進度條都寫入同一筆，避免兩套互相覆寫。
 */
export type SourceReadRecord = {
  opened: boolean
  readPage: number
  totalPages: number
  scrollY: number
  /** 目標頁頂部以下的捲動偏移（像素） */
  offsetInPage: number
  /** 頁內位置 0–1（長圖載入後高度變化時仍準確） */
  offsetRatioInPage: number
}

/** 跨分頁／切換主頁籤保留；資料夾模式另寫入 localStorage（必須 reactive 才會觸發 UI 更新） */
export const localReadSession = reactive({
  readerPages: [] as LocalReaderPage[],
  readerTitle: '',
  readingActive: false,
  pickingSource: false,
  sourceListMode: false,
  folderLabel: '',
  folderTreeUri: '',
  folderSources: [] as LocalReaderSource[],
  currentSourceIndex: -1,
  currentSourcePath: '',
  status: '',
  sessionKind: null as LocalReadSessionKind,
})

export const localReadPageSrcMap = ref(new Map<number, string>())
export const localReadFailedIndices = ref(new Set<number>())

export const folderSourceProgress = reactive<Record<string, SourceReadRecord>>({})

type FolderPersist = {
  folderTreeUri: string
  folderLabel: string
  folderSources: LocalReaderSource[]
  sourceListMode: boolean
  pickingSource: boolean
  currentSourceIndex: number
  currentSourcePath: string
  readerTitle: string
  readingActive: boolean
  readerPages: LocalReaderPage[]
  sourceProgress: Record<string, SourceReadRecord>
}

function readAllFolderProgress(): Record<string, Record<string, SourceReadRecord>> {
  try {
    const raw = localStorage.getItem(PROGRESS_BY_FOLDER_KEY)
    return raw ? (JSON.parse(raw) as Record<string, Record<string, SourceReadRecord>>) : {}
  } catch {
    return {}
  }
}

function writeAllFolderProgress(data: Record<string, Record<string, SourceReadRecord>>) {
  try {
    localStorage.setItem(PROGRESS_BY_FOLDER_KEY, JSON.stringify(data))
  } catch {
    /* 容量不足時略過 */
  }
}

function normalizeRecord(rec: Record<string, unknown> | undefined): SourceReadRecord {
  if (!rec) {
    return { opened: false, readPage: 0, totalPages: 0, scrollY: 0, offsetInPage: 0, offsetRatioInPage: 0 }
  }

  const hasSingleCheckpoint =
    rec.readPage !== undefined ||
    rec.scrollY !== undefined ||
    rec.scrollTop !== undefined

  if (hasSingleCheckpoint) {
    return {
      opened: Boolean(rec.opened),
      readPage: Math.max(0, Number(rec.readPage ?? rec.pageIndex ?? 0)),
      totalPages: Math.max(0, Number(rec.totalPages ?? 0)),
      scrollY: Math.max(0, Number(rec.scrollY ?? rec.scrollTop ?? 0)),
      offsetInPage: Math.max(0, Number(rec.offsetInPage ?? 0)),
      offsetRatioInPage: Math.min(1, Math.max(0, Number(rec.offsetRatioInPage ?? 0))),
    }
  }

  const scrollPos = rec.scrollPos as Record<string, unknown> | undefined
  const dragPos = rec.dragPos as Record<string, unknown> | undefined
  const lastInput = rec.lastInput === 'drag' ? 'drag' : 'scroll'
  const branch = lastInput === 'drag' ? dragPos : scrollPos

  return {
    opened: Boolean(rec.opened),
    readPage: Math.max(0, Number(branch?.pageIndex ?? rec.pageIndex ?? 0)),
    totalPages: Math.max(0, Number(rec.totalPages ?? 0)),
    scrollY: Math.max(0, Number(branch?.scrollTop ?? rec.scrollTop ?? 0)),
    offsetInPage: Math.max(0, Number(branch?.offsetInPage ?? rec.offsetInPage ?? 0)),
    offsetRatioInPage: Math.min(
      1,
      Math.max(0, Number(branch?.offsetRatioInPage ?? rec.offsetRatioInPage ?? 0)),
    ),
  }
}

export function getSourceRecord(path: string): SourceReadRecord {
  return normalizeRecord(folderSourceProgress[path] as Record<string, unknown> | undefined)
}

export function markSourceOpened(path: string, totalPages: number) {
  const prev = getSourceRecord(path)
  folderSourceProgress[path] = {
    ...prev,
    opened: true,
    totalPages: totalPages > 0 ? totalPages : prev.totalPages,
  }
  touchFolderPersist()
}

/** 寫入唯一閱讀斷點（滑動／拖曳鬆手／關閉皆呼叫此函式） */
export function saveSourceReadPosition(
  path: string,
  readPage: number,
  totalPages: number,
  scrollY: number,
  offsetInPage = 0,
  offsetRatioInPage = 0,
) {
  if (!path) return
  const prev = getSourceRecord(path)
  folderSourceProgress[path] = {
    opened: true,
    readPage: Math.max(0, readPage),
    totalPages: totalPages > 0 ? totalPages : prev.totalPages,
    scrollY: Math.max(0, scrollY),
    offsetInPage: Math.max(0, offsetInPage),
    offsetRatioInPage: Math.min(1, Math.max(0, offsetRatioInPage)),
  }
  touchFolderPersist()
}

export function formatSourceProgressLabel(path: string): string | null {
  const rec = getSourceRecord(path)
  if (!rec.opened || rec.totalPages <= 0) return null
  const page = Math.min(rec.readPage + 1, rec.totalPages)
  return `${page}/${rec.totalPages}頁`
}

export function hasSourceReadRecord(path: string): boolean {
  const rec = getSourceRecord(path)
  return rec.opened
}

/** 清除單一篇章的已開啟標記與閱讀進度 */
export function clearSourceReadRecord(path: string) {
  if (!path) return
  delete folderSourceProgress[path]
  touchFolderPersist()
}

export function hasLocalReadSession(): boolean {
  const s = localReadSession
  return (
    s.readingActive ||
    s.pickingSource ||
    s.sourceListMode ||
    s.folderSources.length > 0 ||
    s.currentSourcePath.length > 0
  )
}

function applySourceProgress(data: Record<string, SourceReadRecord> | undefined) {
  for (const key of Object.keys(folderSourceProgress)) {
    delete folderSourceProgress[key]
  }
  if (!data) return
  for (const [path, rec] of Object.entries(data)) {
    folderSourceProgress[path] = normalizeRecord(rec as unknown as Record<string, unknown>)
  }
}

export function loadFolderSourceProgress(treeUri: string) {
  const all = readAllFolderProgress()
  applySourceProgress(all[treeUri])
}

export function persistFolderSourceProgress(treeUri: string) {
  if (!treeUri) return
  const all = readAllFolderProgress()
  all[treeUri] = { ...folderSourceProgress }
  writeAllFolderProgress(all)
}

export function persistFolderSession() {
  if (localReadSession.sessionKind !== 'folder' || !localReadSession.folderTreeUri) return
  persistFolderSourceProgress(localReadSession.folderTreeUri)
  const payload: FolderPersist = {
    folderTreeUri: localReadSession.folderTreeUri,
    folderLabel: localReadSession.folderLabel,
    folderSources: localReadSession.folderSources,
    sourceListMode: localReadSession.sourceListMode,
    pickingSource: localReadSession.pickingSource,
    currentSourceIndex: localReadSession.currentSourceIndex,
    currentSourcePath: localReadSession.currentSourcePath,
    readerTitle: localReadSession.readerTitle,
    readingActive: localReadSession.readingActive,
    readerPages: localReadSession.readerPages,
    sourceProgress: { ...folderSourceProgress },
  }
  try {
    localStorage.setItem(FOLDER_STORAGE_KEY, JSON.stringify(payload))
  } catch {
    /* 容量不足時略過 */
  }
}

export function clearFolderPersist() {
  try {
    localStorage.removeItem(FOLDER_STORAGE_KEY)
  } catch {
    /* ignore */
  }
}

export function restoreFolderSessionFromStorage(): boolean {
  try {
    const raw = localStorage.getItem(FOLDER_STORAGE_KEY)
    if (!raw) return false
    const data = JSON.parse(raw) as FolderPersist
    if (!data.folderTreeUri) return false
    localReadSession.sessionKind = 'folder'
    localReadSession.folderTreeUri = data.folderTreeUri
    localReadSession.folderLabel = decodeFolderDisplayLabel(data.folderLabel ?? '')
    localReadSession.folderSources = data.folderSources ?? []
    localReadSession.sourceListMode = data.sourceListMode ?? false
    localReadSession.pickingSource = data.pickingSource ?? false
    localReadSession.currentSourceIndex = data.currentSourceIndex ?? -1
    localReadSession.currentSourcePath = data.currentSourcePath ?? ''
    localReadSession.readerTitle = data.readerTitle ?? ''
    localReadSession.readingActive = data.readingActive ?? false
    localReadSession.readerPages = data.readerPages ?? []
    loadFolderSourceProgress(data.folderTreeUri)
    if (Object.keys(folderSourceProgress).length === 0 && data.sourceProgress) {
      applySourceProgress(data.sourceProgress)
      persistFolderSourceProgress(data.folderTreeUri)
    }
    return true
  } catch {
    return false
  }
}

export function cancelFolderListMode() {
  const treeUri = localReadSession.folderTreeUri
  if (treeUri) persistFolderSourceProgress(treeUri)
  localReadSession.pickingSource = false
  localReadSession.sourceListMode = false
  localReadSession.readingActive = false
  localReadSession.readerPages = []
  localReadSession.readerTitle = ''
  localReadSession.folderSources = []
  localReadSession.currentSourceIndex = -1
  localReadSession.currentSourcePath = ''
  localReadSession.status = ''
  localReadSession.sessionKind = null
  localReadSession.folderTreeUri = ''
  localReadSession.folderLabel = ''
  clearFolderPersist()
}

export function clearLocalReadSession(clearFolderStorage: boolean) {
  for (const key of Object.keys(folderSourceProgress)) {
    delete folderSourceProgress[key]
  }
  localReadSession.readerPages = []
  localReadSession.readerTitle = ''
  localReadSession.readingActive = false
  localReadSession.pickingSource = false
  localReadSession.sourceListMode = false
  localReadSession.folderLabel = ''
  localReadSession.folderTreeUri = ''
  localReadSession.folderSources = []
  localReadSession.currentSourceIndex = -1
  localReadSession.currentSourcePath = ''
  localReadSession.status = ''
  localReadSession.sessionKind = null
  if (clearFolderStorage) {
    clearFolderPersist()
    try {
      localStorage.removeItem(PROGRESS_BY_FOLDER_KEY)
    } catch {
      /* ignore */
    }
  }
}

export function initLocalReadOnAppLaunch() {
  const restored = restoreFolderSessionFromStorage()
  if (!restored) {
    clearLocalReadSession(false)
    return
  }
  if (localReadSession.sourceListMode && localReadSession.folderSources.length > 0) {
    if (!localReadSession.readingActive) {
      localReadSession.pickingSource = true
      localReadSession.readerTitle = localReadSession.folderLabel
    }
  }
}

export function touchFolderPersist() {
  if (localReadSession.sessionKind === 'folder' && localReadSession.folderTreeUri) {
    persistFolderSession()
  } else if (localReadSession.folderTreeUri) {
    persistFolderSourceProgress(localReadSession.folderTreeUri)
  }
}
