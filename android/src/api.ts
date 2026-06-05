import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type LocalReaderPage = {
  caption: string
  pageId: string
}

export type LocalReaderPages = {
  title: string
  pages: LocalReaderPage[]
}

export type LocalReaderSource = {
  path: string
  label: string
  kind: 'zip' | 'folder'
}

export function getReaderImage(comicId: number, imgUrl: string) {
  return invoke<number[]>('get_reader_image', { comicId, imgUrl })
}

export function listLocalReaderSources(folderPath: string) {
  return invoke<LocalReaderSource[]>('list_local_reader_sources', { folderPath })
}

export function prepareLocalReaderZip(sourceUri: string) {
  return invoke<string>('prepare_local_reader_zip', { sourceUri })
}

export function loadLocalReaderPages(sourcePath: string, sourceKind?: 'zip' | 'folder') {
  return invoke<LocalReaderPages>('load_local_reader_pages', {
    sourcePath,
    sourceKind: sourceKind ?? null,
  })
}

export function pickLocalReaderZip() {
  return invoke<string | null>('pick_local_reader_zip')
}

export function pickLocalReaderFolder() {
  return invoke<string | null>('pick_local_reader_folder')
}

export function pickLocalVideoFile() {
  return invoke<string | null>('pick_local_video_file')
}

export function pickLocalSubtitleFile() {
  return invoke<string | null>('pick_local_subtitle_file')
}

/** ?剜?函??????唾??臬????喟???*/
export type PlayVideoResult = {
  error: string | null
  background?: boolean
}

export type BackgroundPlaybackSession = {
  uri: string
  title: string
  pcHost: string
  pcPort: number
  pcRelPath: string
  subtitleUris: string[]
}

export function getBackgroundPlaybackSession() {
  return invoke<string | null>('get_background_playback_session')
}

export type VideoPlaybackProgress = {
  positionMs: number
  durationMs: number
}

export function getVideoPlaybackProgress(host: string, port: number, relPath: string) {
  return invoke<VideoPlaybackProgress>('get_video_playback_progress', {
    host,
    port,
    relPath,
  })
}

export function stopVideoPlayback() {
  return invoke<void>('stop_video_playback')
}

/** ?剜?函?????隤斤Ⅳ嚗??null嚗?撌脫??PlayVideoResult嚗????乩??澆蝡臭蝙??*/
export function playLocalVideoFile(
  uri: string,
  title?: string | null,
  subtitleUris?: string[] | null,
  options?: { resumeOnly?: boolean },
) {
  return invoke<PlayVideoResult>('play_local_video_file', {
    uri,
    title: title ?? null,
    subtitleUris: subtitleUris ?? null,
    resumeOnly: options?.resumeOnly === true,
  })
}

export function playRemotePcVideo(
  host: string,
  port: number,
  relPath: string,
  title?: string | null,
  subtitleUris?: string[] | null,
  options?: { resumeOnly?: boolean; startPositionMs?: number | null },
) {
  return invoke<PlayVideoResult>('play_remote_pc_video', {
    host,
    port,
    relPath,
    title: title ?? null,
    subtitleUris: subtitleUris ?? null,
    resumeOnly: options?.resumeOnly === true,
    startPositionMs: Math.max(0, options?.startPositionMs ?? 0),
  })
}

export function getLocalReaderImage(pageId: string) {
  return invoke<number[]>('get_local_reader_image', { pageId })
}

export function closeLocalReaderZipSession() {
  return invoke<null>('close_local_reader_zip_session')
}

export type RemoteComicPageItem = {
  index: number
  caption: string
  entry: string
}

export type RemoteComicPagesResult = {
  title: string
  pages: RemoteComicPageItem[]
}

export function fetchRemoteComicPages(host: string, port: number, path: string) {
  return invoke<RemoteComicPagesResult>('fetch_remote_comic_pages', { host, port, path })
}

export function fetchRemoteComicPageImage(
  host: string,
  port: number,
  path: string,
  entry: string,
) {
  return invoke<number[]>('fetch_remote_comic_page_image', { host, port, path, entry })
}

export const MIN_COMIC_REMOTE_API = 8

export function readKoreanTxtCatalog(catalogDir: string) {
  return invoke<string[]>('read_korean_txt_catalog', { catalogDir })
}

export async function listenDownloadTaskEvent(
  handler: (payload: DownloadTaskEvent) => void,
): Promise<UnlistenFn> {
  return listen<DownloadTaskEvent>('download-task-event', (ev) => handler(ev.payload))
}

export type DiscoveredRemotePc = {
  name: string
  hosts: string[]
  port: number
}

export type RemotePcConnectionResult = {
  connected: boolean
  message: string
  connectedHost: string | null
}

export type RemotePcListItem = DiscoveredRemotePc & {
  connected: boolean | null
  message: string
  connectedHost: string | null
}

export type RemotePcDirEntry = {
  name: string
  displayName?: string | null
  isDir: boolean
  size: number | null
  diskFreeBytes?: number | null
  diskTotalBytes?: number | null
}

export type RemotePcBrowseResult = {
  path: string
  pathDisplay?: string | null
  entries: RemotePcDirEntry[]
  remoteApi?: number | null
}

export type RemotePcScanResult = {
  pcs: DiscoveredRemotePc[]
  log: string
}

export function scanLanRemotePcs() {
  return invoke<RemotePcScanResult>('scan_lan_remote_pcs')
}

export function enterRemoteWifiMode() {
  return invoke<string>('enter_remote_wifi_mode')
}

export function leaveRemoteWifiMode() {
  return invoke<void>('leave_remote_wifi_mode')
}

export function testRemotePcConnection(hosts: string[], port: number, skipWifiBind = false) {
  return invoke<RemotePcConnectionResult>('test_remote_pc_connection', {
    hosts,
    port,
    skipWifiBind,
  })
}

export function listRemotePcDirectory(host: string, port: number, path: string) {
  return invoke<RemotePcBrowseResult>('list_remote_pc_directory', { host, port, path })
}

export type RemotePcFileItem = {
  relativePath: string
  size: number
}

export type RemoteTransferFailedItem = {
  path: string
  error: string
}

export type RemoteTransferProgressEvent = {
  phase: string
  fileIndex: number
  fileCount: number
  bytesDone: number
  bytesTotal: number
  speedBps: number
  message: string
  finished: boolean
  error: string | null
  detailLog?: string | null
  succeededPaths?: string[] | null
  failedItems?: RemoteTransferFailedItem[] | null
}

export function pickRemoteTransferDestination() {
  return invoke<string | null>('pick_remote_transfer_destination')
}

export type RemotePcTransferSelection = {
  path: string
  anchorPath: string
}

export function transferRemotePcFiles(
  host: string,
  port: number,
  selections: RemotePcTransferSelection[],
  destTreeUri: string,
) {
  return invoke<null>('transfer_remote_pc_files', { host, port, selections, destTreeUri })
}

export type RemoteUploadPlanItem = {
  sourceUri: string
  destRelativePath: string
  size: number
}

export type RemoteUploadPlan = {
  files: RemoteUploadPlanItem[]
  conflicts: string[]
}

export type RemoteUploadConflictPolicy = 'overwrite' | 'keep_both'

export function pickRemoteUploadFile() {
  return invoke<string | null>('pick_remote_upload_file')
}

export function pickRemoteUploadFolder() {
  return invoke<string | null>('pick_remote_upload_folder')
}

export function planRemotePcUpload(
  host: string,
  port: number,
  pcDestDir: string,
  sourceUri: string,
  kind: 'file' | 'folder',
) {
  return invoke<RemoteUploadPlan>('plan_remote_pc_upload', {
    host,
    port,
    pcDestDir,
    sourceUri,
    kind,
  })
}

export function cancelRemotePcTransfer() {
  return invoke<void>('cancel_remote_pc_transfer')
}

export function uploadRemotePcFiles(
  host: string,
  port: number,
  files: RemoteUploadPlanItem[],
  onConflict: RemoteUploadConflictPolicy,
) {
  return invoke<null>('upload_remote_pc_files', { host, port, files, onConflict })
}

export type RemotePcFileOpAction =
  | 'cut'
  | 'copy'
  | 'paste'
  | 'delete'
  | 'delete_recycle'
  | 'delete_permanent'
  | 'rename'
  | 'mkdir'

export type RemotePcFileOpResult = {
  ok: boolean
  message: string
  clipboardCount?: number | null
}

export function remotePcFileOp(
  host: string,
  port: number,
  action: RemotePcFileOpAction,
  paths: string[],
  destPath: string,
  newName: string,
) {
  return invoke<RemotePcFileOpResult>('remote_pc_file_op', {
    host,
    port,
    action,
    paths,
    destPath,
    newName,
  })
}

export async function listenRemoteTransferProgress(
  handler: (payload: RemoteTransferProgressEvent) => void,
): Promise<import('@tauri-apps/api/event').UnlistenFn> {
  return listen<RemoteTransferProgressEvent>('remote-transfer-progress-event', (ev) =>
    handler(ev.payload),
  )
}
