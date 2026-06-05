// Minimal tauri-specta bindings for Nas Manager Windows PC companion (remote management only).

import { invoke as TAURI_INVOKE } from '@tauri-apps/api/core'

export const commands = {
  async getConfig(): Promise<Config> {
    return await TAURI_INVOKE('get_config')
  },
  async saveConfig(config: Config): Promise<Result<null, CommandError>> {
    try {
      return { status: 'ok', data: await TAURI_INVOKE('save_config', { config }) }
    } catch (e) {
      if (e instanceof Error) throw e
      else return { status: 'error', error: e as CommandError }
    }
  },
  async bindShareRootPath(path: string): Promise<Result<ShareRootBindResult, CommandError>> {
    try {
      return { status: 'ok', data: await TAURI_INVOKE('bind_share_root_path', { path }) }
    } catch (e) {
      if (e instanceof Error) throw e
      else return { status: 'error', error: e as CommandError }
    }
  },
  async getRemoteManagementStatus(): Promise<RemoteManagementStatus> {
    return await TAURI_INVOKE('get_remote_management_status')
  },
  async restartRemoteManagement(): Promise<Result<RemoteManagementStatus, CommandError>> {
    try {
      return { status: 'ok', data: await TAURI_INVOKE('restart_remote_management') }
    } catch (e) {
      if (e instanceof Error) throw e
      else return { status: 'error', error: e as CommandError }
    }
  },
}

export type CommandError = { err_title: string; err_message: string }
export type Result<T, E> = { status: 'ok'; data: T } | { status: 'error'; error: E }

/** Volume GUID 綁定：磁碟代號變更後仍可還原分享路徑 */
export type ShareRootBinding = {
  volumeGuid: string
  relativePath: string
  displayHint: string
}

export type ShareRootBindResult = {
  binding: ShareRootBinding
  resolvedPath: string
}

export type Config = {
  cookie: string
  downloadDir: string
  enableFileLogger: boolean
  downloadFormat: DownloadFormat
  proxyMode: ProxyMode
  proxyHost: string
  proxyPort: number
  comicConcurrency: number
  comicDownloadIntervalSec: number
  imgConcurrency: number
  imgDownloadIntervalSec: number
  downloadShelfIntervalMs: number
  batchDownloadIntervalMs: number
  useOriginalFilename: boolean
  apiDomainMode: ApiDomainMode
  customApiDomain: string
  downloadRetryCount: number
  downloadFailureRestSec: number
  koreanTxtCatalogDir: string
  koreanTxtDuplicateCheckEnabled: boolean
  remoteManagementEnabled: boolean
  remoteManagementDir: string
  remoteManagementShareSlots: number
  remoteManagementDirs: string[]
  remoteManagementShareRoots: ShareRootBinding[]
  remoteManagementPort: number
  remoteManagementToken: string
  remoteManagementDisplayName: string
}

export type RemoteManagementStatus = {
  enabled: boolean
  running: boolean
  httpReachable: boolean
  firewallReady: boolean
  firewallHint: string | null
  port: number
  displayName: string
  shareDir: string
  shareDirs: string[]
  lanAddresses: string[]
  lastError: string | null
}

export type DownloadFormat = 'JpegZipPack' | 'Server2Zip'
export type ProxyMode = 'System' | 'NoProxy' | 'Custom'
export type ApiDomainMode = 'Default' | 'Custom'
