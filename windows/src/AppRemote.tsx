import { defineComponent, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import {
  commands,
  type Config,
  type RemoteManagementStatus,
  type ShareRootBinding,
} from './bindings.ts'

function clampSlots(n: number): number {
  if (!Number.isFinite(n)) return 3
  return Math.min(16, Math.max(1, Math.round(n)))
}

const EMPTY_BINDING: ShareRootBinding = {
  volumeGuid: '',
  relativePath: '',
  displayHint: '',
}

function isBindingEmpty(b: ShareRootBinding): boolean {
  return !b.volumeGuid?.trim()
}

function ensureShareRoots(config: Config): ShareRootBinding[] {
  const slots = clampSlots(config.remoteManagementShareSlots ?? 3)
  const roots = [...(config.remoteManagementShareRoots ?? [])]
  while (roots.length < slots) roots.push({ ...EMPTY_BINDING })
  return roots.slice(0, slots)
}

export default defineComponent({
  name: 'AppRemote',
  setup() {
    const config = ref<Config | null>(null)
    const status = ref<RemoteManagementStatus | null>(null)
    const shareRoots = ref<ShareRootBinding[]>([])
    const loading = ref(true)
    const saving = ref(false)
    const message = ref('')
    let timer: ReturnType<typeof setInterval> | null = null
    let autoSaveTimer: ReturnType<typeof setTimeout> | null = null

    function syncShareRootsFromConfig() {
      if (!config.value) return
      shareRoots.value = ensureShareRoots(config.value)
    }

    function shareRootLabel(index: number): string {
      const binding = shareRoots.value[index]
      if (!binding || isBindingEmpty(binding)) return '（未指定）'
      const resolved = status.value?.shareDirs?.[index]
      if (resolved) return resolved
      if (binding.displayHint) return binding.displayHint
      if (binding.relativePath) {
        return `Volume {${binding.volumeGuid}} / ${binding.relativePath}`
      }
      return `Volume {${binding.volumeGuid}}`
    }

    function applyShareRootsToConfig() {
      if (!config.value) return
      const cleaned = shareRoots.value.filter((b) => !isBindingEmpty(b))
      config.value.remoteManagementShareRoots = cleaned
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
      message.value = ''
      try {
        config.value = await commands.getConfig()
        if (config.value.remoteManagementShareSlots == null) {
          config.value.remoteManagementShareSlots = 3
        }
        if (!config.value.remoteManagementShareRoots) {
          config.value.remoteManagementShareRoots = []
        }
        syncShareRootsFromConfig()
        await refreshStatus()
      } catch (err) {
        config.value = null
        message.value =
          err instanceof Error ? `載入設定失敗：${err.message}` : '載入設定失敗，請重新啟動程式'
      } finally {
        loading.value = false
      }
    }

    async function save(silent = false) {
      if (!config.value) return
      applyShareRootsToConfig()
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
      shareRoots,
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
        syncShareRootsFromConfig()
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
      if (typeof picked !== 'string' || picked.length === 0) return

      const result = await commands.bindShareRootPath(picked)
      if (result.status !== 'ok') {
        message.value = result.error.err_message || '綁定分享路徑失敗'
        return
      }

      const next = [...shareRoots.value]
      while (next.length <= index) next.push({ ...EMPTY_BINDING })
      next[index] = result.data.binding
      shareRoots.value = next
    }

    function clearShareDir(index: number) {
      const next = [...shareRoots.value]
      next[index] = { ...EMPTY_BINDING }
      shareRoots.value = next
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

              {shareRoots.value.map((binding, index) => (
                <div
                  key={index}
                  style="display:grid;grid-template-columns:1fr auto auto;gap:8px;margin-bottom:8px;align-items:center;"
                >
                  <input
                    value={shareRootLabel(index)}
                    readonly
                    style="padding:8px 10px;border-radius:8px;border:1px solid #444;background:#121212;color:#e8e8e8;"
                  />
                  <button onClick={() => void pickShareDir(index)} style="padding:8px 12px;">
                    指定 #{index + 1}
                  </button>
                  <button
                    disabled={isBindingEmpty(binding)}
                    onClick={() => clearShareDir(index)}
                    style="padding:8px 12px;"
                  >
                    清除
                  </button>
                </div>
              ))}

              <p style="margin:0 0 10px;font-size:12px;color:#888;">
                分享路徑以 Volume GUID 綁定，磁碟代號變更後仍可還原。更換路徑後會自動儲存；若服務已在執行，請按「重新啟動服務」。
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
