import { testRemotePcConnection } from './api'
import { candidateHostsForStreamJob, rememberRemotePcHosts } from './remotePcHostRegistry'
import { looksLikeTailscaleHost } from './remotePcName'
import type { RemoteStreamJob } from './videoStreamStore'

export type ResolvedStreamJob = RemoteStreamJob & { host: string }

/**
 * 播放／閱讀串流前，在區網與 Tailscale 等候選 IP 中找出目前可連的 host。
 * 成功時會把可達 host 寫回登錄，方便下次優先。
 */
export async function resolveStreamJobHost(job: RemoteStreamJob): Promise<ResolvedStreamJob> {
  const candidates = candidateHostsForStreamJob(job)
  if (candidates.length === 0) return { ...job }

  let lastError = ''
  for (const host of candidates) {
    try {
      const skipWifi = looksLikeTailscaleHost(host)
      const result = await testRemotePcConnection([host], job.port, skipWifi)
      if (result.connected) {
        const connected = (result.connectedHost ?? host).trim()
        if (job.pcName?.trim()) {
          rememberRemotePcHosts(job.pcName, [connected, ...candidates], job.port)
        } else {
          rememberRemotePcHosts(connected, [connected, ...candidates], job.port)
        }
        if (connected !== job.host) {
          return { ...job, host: connected }
        }
        return { ...job, host: connected }
      }
      lastError = result.message || '無法連線'
    } catch (e) {
      lastError = e instanceof Error ? e.message : String(e)
    }
  }

  // 全部失敗：仍回傳原 job，讓後續播放拋出原本錯誤；附帶提示用訊息
  if (lastError && candidates.length > 1) {
    console.warn(
      `[stream-host] 無法透過候選 IP 連線 ${job.pcName ?? job.host}（已試：${candidates.join(', ')}）：${lastError}`,
    )
  }
  return { ...job }
}
