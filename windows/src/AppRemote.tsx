import { defineComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import { commands, type Config, type RemoteManagementStatus } from './bindings.ts'

function clampSlots(n: number): number {
  if (!Number.isFinite(n)) return 3
  return Math.min(16, Math.max(1, Math.round(n)))
}

function ensureShareDirs(config: Config): string[] {
  const slots = clampSlots(config.remoteManagementShareSlots ?? 3)
  const dirs = [...(config.remoteManagementDirs ?? [])]
  while (dirs.length < slots) dirs.push('')
  return dirs.slice(0, slots)
}

export default defineComponent({
  name: 'AppRemote',
  setup() {
    const config = ref<Config | null>(null)
    const status = ref<RemoteManagementStatus | null>(null)
    const shareDirs = ref<string[]>([])
    const loading = ref(true)
    const saving = ref(false)
    const message = ref('')
    let timer: ReturnType<typeof setInterval> | null = null
    let autoSaveTimer: ReturnType<typeof setTimeout> | null = null

    function syncShareDirsFromConfig() {
      if (!config.value) return
      shareDirs.value = ensureShareDirs(config.value)
    }

    function applyShareDirsToConfig() {
      if (!config.value) return
      const cleaned = shareDirs.value.map((d) => d.trim()).filter((d) => d.length > 0)
      config.value.remoteManagementDirs = cleaned
      config.value.remoteManagementDir = cleaned[0] ?? ''
    }

    async function refreshStatus() {
      try {
        status.value = await commands.getRemoteManagementStatus()
      } catch {
        status.value = null
      }
    }

    async function load() {
      loading.value = true
      try {
        config.value = await commands.getConfig()
        if (config.value.remoteManagementShareSlots == null) {
          config.value.remoteManagementShareSlots = 3
        }
        syncShareDirsFromConfig()
        await refreshStatus()
      } finally {
        loading.value = false
      }
    }

    async function save(silent = false) {
      if (!config.value) return
      applyShareDirsToConfig()
      config.value.remoteManagementShareSlots = clampSlots(
        config.value.remoteManagementShareSlots ?? 3,
      )
      saving.value = true
      if (!silent) message.value = ''
      try {
        const result = await commands.saveConfig(config.value)
        if (result.status === 'ok') {
          message.value = silent ? '已自動儲存' : '設定已儲存'
          await refreshStatus()
        } else {
          message.value = result.error.err_message || '儲存失敗'
        }
      } finally {
        saving.value = false
      }
    }

    function scheduleAutoSave() {
      if (autoSaveTimer != null) clearTimeout(autoSaveTimer)
      autoSaveTimer = window.setTimeout(() => {
        autoSaveTimer = null
        void save(true)
      }, 400)
    }

    watch(
      shareDirs,
      () => {
        if (loading.value) return
        scheduleAutoSave()
      },
      { deep: true },
    )

    watch(
      () => config.value?.remoteManagementShareSlots,
      () => {
        if (!config.value || loading.value) return
        config.value.remoteManagementShareSlots = clampSlots(
          config.value.remoteManagementShareSlots ?? 3,
        )
        syncShareDirsFromConfig()
        scheduleAutoSave()
      },
    )

    async function restartRemote() {
      message.value = ''
      const result = await commands.restartRemoteManagement()
      if (result.status === 'ok') {
        status.value = result.data
        message.value = '已重新啟動遠端服務'
      } else {
        message.value = result.error.err_message || '重新啟動失敗'
      }
    }

    async function pickShareDir(index: number) {
      if (!config.value) return
      const picked = await open({ directory: true, multiple: false })
      if (typeof picked === 'string' && picked.length > 0) {
        const next = [...shareDirs.value]
        while (next.length <= index) next.push('')
        next[index] = picked
        shareDirs.value = next
      }
    }

    function clearShareDir(index: number) {
      const next = [...shareDirs.value]
      next[index] = ''
      shareDirs.value = next
    }

    onMounted(async () => {
      await load()
      timer = setInterval(() => {
        void refreshStatus()
      }, 2000)
    })

    onBeforeUnmount(() => {
      if (timer) clearInterval(timer)
      if (autoSaveTimer != null) clearTimeout(autoSaveTimer)
    })

    return () => (
      <div style="height:100vh;padding:20px;background:#141414;color:#f5f5f5;font-family:'Segoe UI',sans-serif;">
        <div style="max-width:760px;margin:0 auto;border:1px solid #333;border-radius:12px;padding:18px;background:#1f1f1f;">
          <h1 style="margin:0 0 8px;font-size:22px;">Nas Manager（PC）遠端管理</h1>
          <p style="margin:0 0 14px;color:#bfbfbf;">
            本程式為 Nas Manager 的 Windows 配套端，僅提供區網遠端管理（分享資料夾、HTTP 串流服務）相關設定。
          </p>

          {loading.value || !config.value ? (
            <p>載入中…</p>
          ) : (
            <>
              <label style="display:flex;align-items:center;gap:8px;margin-bottom:10px;">
                <input
                  type="checkbox"
                  checked={config.value.remoteManagementEnabled}
                  onChange={(e) => {
                    config.value!.remoteManagementEnabled = (e.target as HTMLInputElement).checked
                    void save(true)
                  }}
                />
                <span>開啟遠端管理（區網）</span>
              </label>

              <div
                style="display:flex;align-items:center;gap:8px;margin-bottom:10px;flex-wrap:wrap;"
              >
                <span>分享資料夾數量：</span>
                <input
                  type="number"
                  min={1}
                  max={16}
                  value={config.value.remoteManagementShareSlots ?? 3}
                  onInput={(e) => {
                    const n = Number((e.target as HTMLInputElement).value)
                    config.value!.remoteManagementShareSlots = clampSlots(n)
                  }}
                  style="width:72px;padding:6px 8px;border-radius:8px;border:1px solid #444;background:#121212;color:#e8e8e8;"
                />
                <span style="color:#888;font-size:12px;">（多個時，手機根目錄會列出各分享名稱）</span>
              </div>

              {shareDirs.value.map((dir, index) => (
                <div
                  key={index}
                  style="display:grid;grid-template-columns:1fr auto auto;gap:8px;margin-bottom:8px;align-items:center;"
                >
                  <input
                    value={dir || '（未指定）'}
                    readonly
                    style="padding:8px 10px;border-radius:8px;border:1px solid #444;background:#121212;color:#e8e8e8;"
                  />
                  <button onClick={() => void pickShareDir(index)} style="padding:8px 12px;">
                    指定 #{index + 1}
                  </button>
                  <button
                    disabled={!dir}
                    onClick={() => clearShareDir(index)}
                    style="padding:8px 12px;"
                  >
                    清除
                  </button>
                </div>
              ))}

              <p style="margin:0 0 10px;font-size:12px;color:#888;">
                更換路徑後會自動儲存；若服務已在執行，請按「重新啟動服務」套用新分享目錄。
              </p>

              <div style="display:flex;gap:8px;align-items:center;margin-bottom:12px;flex-wrap:wrap;">
                <span>連接埠：</span>
                <input
                  type="number"
                  value={config.value.remoteManagementPort}
                  onInput={(e) => {
                    const n = Number((e.target as HTMLInputElement).value)
                    config.value!.remoteManagementPort = Number.isFinite(n) ? n : 8765
                  }}
                  style="width:110px;padding:6px 8px;border-radius:8px;border:1px solid #444;background:#121212;color:#e8e8e8;"
                />
                <button disabled={saving.value} onClick={() => void save(false)} style="padding:8px 12px;">
                  {saving.value ? '儲存中…' : '儲存設定'}
                </button>
                <button onClick={() => void restartRemote()} style="padding:8px 12px;">
                  重新啟動服務
                </button>
              </div>
            </>
          )}

          <div style="font-size:13px;line-height:1.6;color:#d9d9d9;">
            <div>服務狀態：{status.value?.running ? '執行中' : '未執行'}</div>
            <div>顯示名稱：{status.value?.displayName ?? '-'}</div>
            <div>分享目錄：{status.value?.shareDir ?? '-'}</div>
            <div>區網 IP：{status.value?.lanAddresses?.join('、') || '-'}</div>
            <div>防火牆：{status.value?.firewallReady ? '已就緒' : '未確認'}</div>
            {status.value?.lastError && <div style="color:#ff7875;">錯誤：{status.value.lastError}</div>}
            {message.value && <div style="margin-top:8px;color:#95de64;">{message.value}</div>}
          </div>
        </div>
      </div>
    )
  },
})
