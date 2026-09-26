/** 客戶端剪貼簿：支援跨 PC 複製／剪下後貼上（UI 不變，貼上時依來源 PC 分流）。 */

export type RemotePcClipboardMode = 'copy' | 'cut'

export type RemotePcClipboardEntry = {
  path: string
  anchorPath: string
}

export type RemotePcClipboard = {
  mode: RemotePcClipboardMode
  sourceHost: string
  sourcePort: number
  entries: RemotePcClipboardEntry[]
  savedAt: number
}

const STORAGE_KEY = 'nas-remote-pc-clipboard-v1'

export function saveRemotePcClipboard(clip: RemotePcClipboard): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(clip))
  } catch {
    /* ignore quota */
  }
}

export function loadRemotePcClipboard(): RemotePcClipboard | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as RemotePcClipboard
    if (
      (parsed.mode !== 'copy' && parsed.mode !== 'cut') ||
      !parsed.sourceHost ||
      !parsed.sourcePort ||
      !Array.isArray(parsed.entries) ||
      parsed.entries.length === 0
    ) {
      return null
    }
    return parsed
  } catch {
    return null
  }
}

export function clearRemotePcClipboard(): void {
  try {
    localStorage.removeItem(STORAGE_KEY)
  } catch {
    /* ignore */
  }
}

export function isSameRemotePc(clip: RemotePcClipboard, host: string, port: number): boolean {
  return clip.sourceHost.trim() === host.trim() && clip.sourcePort === port
}
